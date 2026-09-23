// backend/crates/services/src/storage_service.rs
use crate::config::StorageConfig;
use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::error::ProvideErrorMetadata;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::{Client, error::SdkError};
use uuid::Uuid;

/// Which bucket an operation addresses.
///
/// Every entry point below takes this rather than a bucket name, so a caller
/// cannot reach a bucket by spelling its name: the only two values that exist
/// are the two the instance is configured with, and resolving one to a name is
/// private to this module. That is what makes a cross-bucket write a compile
/// error rather than a code-review question.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bucket {
    Attachments,
    Avatars,
}

/// Which kind of entity owns an avatar. Part of the key, so it is an enum
/// rather than a string for the same reason `Bucket` is: the set is closed and
/// a typo in it would write an object nothing can ever find again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AvatarKind {
    User,
    Team,
    Workspace,
}

impl AvatarKind {
    /// The path segment, pluralised to match the table names.
    fn segment(self) -> &'static str {
        match self {
            AvatarKind::User => "users",
            AvatarKind::Team => "teams",
            AvatarKind::Workspace => "workspaces",
        }
    }
}

/// Local to this service, as every other service here does, rather than a
/// shared `AppError`: the API layer decides what a storage failure means for a
/// given route, and that decision differs between "the avatar you asked for is
/// gone" and "the object store is down".
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("object not found")]
    NotFound,

    /// A bucket the instance is configured with did not answer a HeadBucket.
    /// Carries the bucket name deliberately: this one is only ever rendered
    /// into a startup error for an operator, never into an HTTP response.
    #[error("bucket {bucket} is unreachable: {message}")]
    BucketUnavailable { bucket: String, message: String },

    /// Everything the provider rejected or failed to answer. The SDK's own
    /// error types are not carried through: they are large, they vary by
    /// operation, and their `Display` includes request ids that belong in a
    /// log line rather than in a type every caller has to match on.
    #[error("storage request failed: {0}")]
    Backend(String),

    /// The object is larger than the caller is willing to hold in memory.
    /// See `get`, which is a buffering read by design.
    #[error("object is larger than the {limit} byte read limit")]
    TooLarge { limit: usize },

    /// The credential was refused by the provider.
    ///
    /// This is what any 403 that cannot be attributed to a quota becomes, and
    /// it is the safe default of the two: it points an operator at the
    /// credential's permissions, which is where the problem is whenever the
    /// quota heuristic in `classify_refusal` does not fire.
    #[error("object storage refused the credential: {detail}")]
    AccessDenied { detail: String },

    /// The bucket is full: a storage quota refused the write.
    ///
    /// Separated from `Backend` because it is the one write failure an
    /// operator can actually fix, and because every provider reports it as
    /// `AccessDenied` - so without this it reads as a permissions bug in our
    /// own code, and the API answers "service unavailable" for a condition
    /// that no amount of waiting will clear.
    #[error("storage quota reached: {detail}")]
    QuotaExceeded { detail: String },
}

/// The object store, as one enum rather than a trait.
///
/// There is exactly one runtime implementation and there is not going to be a
/// second: S3 is the only backend, and a local-filesystem mode does not exist.
/// A trait would therefore buy nothing but a `dyn`-safety constraint, and
/// inherent `async fn` on an enum keeps the futures concrete and `Send`
/// without an attribute macro to make them so.
pub enum ObjectStore {
    S3(S3Store),

    /// In-memory, for tests. Not reachable from a release build: the variant
    /// itself does not exist outside `cfg(test)`, so there is nothing to
    /// construct and nothing to accidentally configure an instance into.
    #[cfg(test)]
    Memory(MemoryStore),
}

impl ObjectStore {
    /// Stores `body` under `key`. Overwriting is expected, not an error:
    /// avatars are versioned in the key, so a genuine overwrite only happens
    /// when the same version is written twice.
    pub async fn put(
        &self,
        bucket: Bucket,
        key: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<(), StorageError> {
        match self {
            ObjectStore::S3(store) => store.put(bucket, key, body, content_type).await,
            #[cfg(test)]
            ObjectStore::Memory(store) => store.put(bucket, key, body),
        }
    }

    /// Reads an object whole, refusing anything past `limit`.
    ///
    /// Buffering rather than streaming is a deliberate scope choice: the only
    /// caller today is the avatar endpoint, whose objects this service
    /// re-encoded itself and which are a few tens of kilobytes. Attachments
    /// (Phase 4) will be operator-supplied and unbounded, and will need a
    /// streaming read that hands the `ByteStream` to the response body rather
    /// than collecting it - adding one is the right move at that point, not
    /// stretching this to cover a case with no code behind it yet.
    pub async fn get(
        &self,
        bucket: Bucket,
        key: &str,
        limit: usize,
    ) -> Result<Vec<u8>, StorageError> {
        match self {
            ObjectStore::S3(store) => store.get(bucket, key, limit).await,
            #[cfg(test)]
            ObjectStore::Memory(store) => store.get(bucket, key, limit),
        }
    }

    /// Deletes one object. S3 treats deleting a key that does not exist as a
    /// success, and so does this: the caller's intent is that the object be
    /// gone, and it is.
    pub async fn delete(&self, bucket: Bucket, key: &str) -> Result<(), StorageError> {
        match self {
            ObjectStore::S3(store) => store.delete(bucket, key).await,
            #[cfg(test)]
            ObjectStore::Memory(store) => store.delete(bucket, key),
        }
    }

    /// Deletes every object under `prefix`, paging through the listing.
    ///
    /// One `DeleteObject` per key rather than a batched `DeleteObjects`: the
    /// application grant made by `ops/garage/bootstrap.py` covers the single-object
    /// verb only, and keeping the code to what the policy grants is what makes
    /// the policy readable as the real bound on this service. The batch verb
    /// would also fail differently per provider, which is exactly the kind of
    /// portability seam this layer exists to avoid.
    pub async fn delete_prefix(&self, bucket: Bucket, prefix: &str) -> Result<(), StorageError> {
        match self {
            ObjectStore::S3(store) => store.delete_prefix(bucket, prefix).await,
            #[cfg(test)]
            ObjectStore::Memory(store) => store.delete_prefix(bucket, prefix),
        }
    }

    /// `HeadBucket` on both buckets. Called at boot so a misconfigured or
    /// unprovisioned instance fails to start rather than failing on a user's
    /// first upload.
    pub async fn health(&self) -> Result<(), StorageError> {
        match self {
            ObjectStore::S3(store) => store.health().await,
            #[cfg(test)]
            ObjectStore::Memory(_) => Ok(()),
        }
    }
}

/// The S3-backed store.
pub struct S3Store {
    client: Client,
    bucket_attachments: String,
    bucket_avatars: String,
    /// Already normalised by `StorageConfig::normalized_key_prefix`: empty, or
    /// ending in exactly one `/`.
    key_prefix: String,
}

impl S3Store {
    /// Builds the client from our own configuration, with no credential chain.
    ///
    /// `aws-config` is deliberately not a dependency. Its provider chain would
    /// fall through to the EC2 instance metadata service and the ECS container
    /// credential endpoint when our variables are absent, which means a
    /// misconfigured instance would issue HTTP requests to link-local
    /// addresses at boot instead of failing. Credentials here come from our
    /// environment or the process does not start.
    pub fn new(config: &StorageConfig) -> Self {
        let credentials = Credentials::new(
            config.access_key_id.clone(),
            config.secret_access_key.clone(),
            // No session token: the application credential is a long-lived
            // key pair issued by `bootstrap.sh` or by the operator's provider.
            None,
            None,
            "pwnforge-env",
        );

        let mut builder = aws_sdk_s3::Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new(config.region.clone()))
            .credentials_provider(credentials)
            .force_path_style(config.force_path_style);

        // Absent for AWS, where the endpoint is derived from the region.
        // Present for Garage, R2, Scaleway.
        if let Some(endpoint) = &config.endpoint {
            builder = builder.endpoint_url(endpoint);
        }

        S3Store {
            client: Client::from_conf(builder.build()),
            bucket_attachments: config.bucket_attachments.clone(),
            bucket_avatars: config.bucket_avatars.clone(),
            key_prefix: config.normalized_key_prefix(),
        }
    }

    /// The only place a `Bucket` becomes a name. Private, so no caller can
    /// obtain a bucket name to pass somewhere else.
    fn bucket_name(&self, bucket: Bucket) -> &str {
        match bucket {
            Bucket::Attachments => &self.bucket_attachments,
            Bucket::Avatars => &self.bucket_avatars,
        }
    }

    fn prefixed(&self, key: &str) -> String {
        format!("{}{}", self.key_prefix, key)
    }

    async fn put(
        &self,
        bucket: Bucket,
        key: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<(), StorageError> {
        self.client
            .put_object()
            .bucket(self.bucket_name(bucket))
            .key(self.prefixed(key))
            .content_type(content_type)
            .body(ByteStream::from(body))
            .send()
            .await
            .map_err(backend_error)?;

        Ok(())
    }

    async fn get(&self, bucket: Bucket, key: &str, limit: usize) -> Result<Vec<u8>, StorageError> {
        let response = self
            .client
            .get_object()
            .bucket(self.bucket_name(bucket))
            .key(self.prefixed(key))
            .send()
            .await
            .map_err(|err| match &err {
                SdkError::ServiceError(service) if service.err().is_no_such_key() => {
                    StorageError::NotFound
                }
                _ => backend_error(err),
            })?;

        // Checked before collecting, so an oversized object is refused without
        // ever being held in memory. `content_length` is advisory - it is what
        // the provider claims - so the collected length is checked again below.
        if let Some(length) = response.content_length()
            && length > limit as i64
        {
            return Err(StorageError::TooLarge { limit });
        }

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|err| StorageError::Backend(err.to_string()))?
            .into_bytes();

        if bytes.len() > limit {
            return Err(StorageError::TooLarge { limit });
        }

        Ok(bytes.to_vec())
    }

    async fn delete(&self, bucket: Bucket, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(self.bucket_name(bucket))
            .key(self.prefixed(key))
            .send()
            .await
            .map_err(backend_error)?;

        Ok(())
    }

    async fn delete_prefix(&self, bucket: Bucket, prefix: &str) -> Result<(), StorageError> {
        let name = self.bucket_name(bucket);
        let mut continuation: Option<String> = None;

        loop {
            let response = self
                .client
                .list_objects_v2()
                .bucket(name)
                .prefix(self.prefixed(prefix))
                .set_continuation_token(continuation.take())
                .send()
                .await
                .map_err(backend_error)?;

            for object in response.contents() {
                // The listing returns keys already carrying `key_prefix`, and
                // `delete` would add it a second time. Delete by the absolute
                // key the listing gave us instead of round-tripping through
                // `prefixed`.
                if let Some(key) = object.key() {
                    self.client
                        .delete_object()
                        .bucket(name)
                        .key(key)
                        .send()
                        .await
                        .map_err(backend_error)?;
                }
            }

            if response.is_truncated().unwrap_or(false) {
                continuation = response.next_continuation_token().map(str::to_owned);
                // A truncated listing with no token would otherwise spin
                // forever re-reading the same first page.
                if continuation.is_none() {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(())
    }

    async fn health(&self) -> Result<(), StorageError> {
        for bucket in [Bucket::Attachments, Bucket::Avatars] {
            let name = self.bucket_name(bucket);

            self.client
                .head_bucket()
                .bucket(name)
                .send()
                .await
                .map_err(|err| StorageError::BucketUnavailable {
                    bucket: name.to_owned(),
                    // HeadBucket answers with a bare status and no body, so
                    // the SDK has nothing to build a message from: without
                    // this, every failure renders as "unhandled error".
                    //
                    // The two branches are not as separable as they look. A
                    // correctly scoped credential has no permission on a
                    // bucket outside its policy, so a bucket that does not
                    // exist and a bucket that was never granted both come back
                    // as 403, and `NotFound` is only reachable for a
                    // credential broad enough to see the bucket is absent.
                    // The message says so rather than implying the operator
                    // has been told which one it is.
                    message: match &err {
                        SdkError::ServiceError(service) if service.err().is_not_found() => {
                            "no such bucket - create it, or fix the name".to_owned()
                        }
                        SdkError::ServiceError(_) => {
                            "refused - the bucket does not exist, is named differently, or is \
                             not covered by the policy attached to this credential"
                                .to_owned()
                        }
                        other => sdk_message(other),
                    },
                })?;
        }

        Ok(())
    }
}

/// Builds the public URL for an avatar.
///
/// Takes a key and no `Bucket`. That is the point: there is no parameter to
/// pass `Bucket::Attachments` to, so no amount of caller confusion can produce
/// a public URL for an attachment. Attachments are served only through the
/// permission-checking Axum endpoint, and this function is the reason that
/// cannot be bypassed by accident.
///
/// Returns `None` when no CDN is configured, which is the default: the caller
/// then falls back to the API's own avatar route.
pub fn avatar_public_url(config: &StorageConfig, key: &str) -> Option<String> {
    let base = config.public_base_url.as_ref()?;
    let prefix = config.normalized_key_prefix();

    Some(format!("{base}/{prefix}{key}"))
}

/// The two renditions an upload produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AvatarSize {
    /// Profile headers and cards.
    Large,
    /// Inline lists, comment rows, member tables.
    Small,
}

impl AvatarSize {
    /// The square edge, in pixels.
    pub fn pixels(self) -> u32 {
        match self {
            AvatarSize::Large => 256,
            AvatarSize::Small => 64,
        }
    }
}

/// Key for an avatar object:
/// `avatars/{users|teams|workspaces}/{id}/{version}-{px}.webp`.
///
/// Pure, and derived from `(kind, id, version, size)` alone. No row stores a
/// key or a URL: the database holds `avatar_version` and nothing else, so a
/// row can only ever name the objects belonging to its own entity. A stored
/// key would let a write anywhere in the application point a record at an
/// arbitrary object in the bucket, which is a much larger surface than the
/// cache-busting convenience it would buy.
///
/// The `-{px}` suffix is the one addition to the originally specified scheme.
/// An upload produces two renditions and one key cannot hold two objects, so
/// the size has to appear somewhere; as a suffix it keeps both renditions
/// under the same version prefix, which is what lets the previous version be
/// deleted with a single prefix scan.
///
/// The leading `avatars/` segment is redundant with the bucket until someone
/// sets `PWNFORGE_S3_KEY_PREFIX` and shares one bucket between both kinds of
/// object, at which point it is what keeps the two keyspaces apart.
pub fn avatar_key(kind: AvatarKind, id: Uuid, version: Uuid, size: AvatarSize) -> String {
    format!(
        "avatars/{}/{}/{}-{}.webp",
        kind.segment(),
        id,
        version,
        size.pixels()
    )
}

/// Both renditions of one version, for deleting a superseded avatar.
pub fn avatar_version_prefix(kind: AvatarKind, id: Uuid, version: Uuid) -> String {
    format!("avatars/{}/{}/{}-", kind.segment(), id, version)
}

/// Every version ever written for one entity, for deleting the lot when the
/// entity is deleted.
pub fn avatar_prefix(kind: AvatarKind, id: Uuid) -> String {
    format!("avatars/{}/{}/", kind.segment(), id)
}

/// Key for a challenge attachment: `{workspace_id}/{challenge_id}/{attachment_id}`.
///
/// Identifiers, never slugs. Slugs are mutable and S3 has no rename: renaming
/// a workspace would leave every attachment under the old slug, reachable by
/// nothing and deleted by nothing. Ids never change, so a key derived from
/// them stays correct for the life of the object.
///
/// Unused today - challenges are Phase 4 and no attachment exists yet. It
/// lives here rather than waiting so the bucket that `bootstrap.sh`
/// provisions has a documented keyspace, and so the slug-based scheme is not
/// reinvented when the feature lands.
#[allow(dead_code, reason = "keyspace for challenge attachments, Phase 4")]
pub fn attachment_key(workspace_id: Uuid, challenge_id: Uuid, attachment_id: Uuid) -> String {
    format!("{workspace_id}/{challenge_id}/{attachment_id}")
}

/// Flattens an SDK error, logging the full error first.
///
/// The detail is genuinely useful - it names the bucket, the operation and the
/// provider's request id - and genuinely must not reach a client, which is why
/// it goes to the log here and only a summary goes into the error.
fn backend_error<E, R>(err: SdkError<E, R>) -> StorageError
where
    E: std::error::Error + ProvideErrorMetadata + Send + Sync + 'static,
    R: std::fmt::Debug,
{
    tracing::error!(error = ?err, "object storage request failed");

    match &err {
        SdkError::ServiceError(service) => match classify_refusal(service.err()) {
            Refusal::Quota(detail) => StorageError::QuotaExceeded { detail },
            Refusal::Denied(detail) => StorageError::AccessDenied { detail },
            Refusal::Other => StorageError::Backend(sdk_message(&err)),
        },
        _ => StorageError::Backend(sdk_message(&err)),
    }
}

/// What a provider's refusal was actually about.
enum Refusal {
    /// The bucket is full. Only ever returned when the provider said so.
    Quota(String),
    /// The credential was refused. The default reading of any 403.
    Denied(String),
    /// Not a refusal at all: a 404, a 500, a malformed response.
    Other,
}

/// Decides what a service error means, in a fixed order.
///
/// The order is the point, so it is written as one branch rather than two
/// predicates:
///
/// 1. A code that names a quota outright is a quota.
/// 2. A 403 whose message names a quota is a quota, **best effort**.
/// 3. Any other 403 is a permission refusal.
/// 4. Everything else is an opaque backend failure.
///
/// Step 3 is the default, and that is deliberate. Step 2 is the only
/// provider-specific rule in this file, and ADR 0001 exists partly to keep
/// such rules out: S3 has no quota concept, so Garage smuggles one inside
/// `AccessDenied` with an explanatory message, while AWS and R2 have no
/// per-bucket quota at all and will never produce it. Matching on prose is
/// therefore a Garage-shaped convenience, not a portable contract.
///
/// Which is why it can only ever *upgrade* a refusal from "denied" to "full",
/// never the reverse. If the message is absent, translated, reworded upstream,
/// or simply from a provider that has no quotas, the rule does not fire and
/// the caller gets `AccessDenied` - a wrong-but-safe answer that sends the
/// operator to look at the credential's permissions. A misfire in the other
/// direction would tell them to raise a quota that does not exist while their
/// policy is the real problem.
fn classify_refusal<E: ProvideErrorMetadata>(err: &E) -> Refusal {
    let code = err.code().unwrap_or_default();
    let message = err.message().unwrap_or_default();

    let detail = || {
        if message.is_empty() {
            code.to_owned()
        } else {
            message.to_owned()
        }
    };

    // 1. The provider named it. Portable across anything that does this.
    if code.to_ascii_lowercase().contains("quotaexceeded") {
        return Refusal::Quota(detail());
    }

    let is_denial =
        code.eq_ignore_ascii_case("AccessDenied") || code.eq_ignore_ascii_case("Forbidden");

    if is_denial {
        // 2. Garage-specific, best effort. See the note above.
        if message.to_ascii_lowercase().contains("quota") {
            return Refusal::Quota(detail());
        }

        // 3. The default for every denial we cannot attribute to a quota.
        return Refusal::Denied(detail());
    }

    // 4. Not a refusal.
    Refusal::Other
}

fn sdk_message<E, R>(err: &SdkError<E, R>) -> String
where
    E: std::error::Error + ProvideErrorMetadata,
{
    match err {
        // The provider's own message, when there is one. `Display` on these
        // error types is frequently just "unhandled error (SomeCode)", which
        // names the code and drops the sentence explaining it.
        SdkError::ServiceError(service) => match service.err().message() {
            Some(message) if !message.is_empty() => {
                format!(
                    "{} ({})",
                    message,
                    service.err().code().unwrap_or("no code")
                )
            }
            _ => service.err().to_string(),
        },
        SdkError::TimeoutError(_) => "request timed out".to_owned(),
        SdkError::DispatchFailure(_) => "could not reach the object store".to_owned(),
        SdkError::ConstructionFailure(_) => "malformed storage request".to_owned(),
        SdkError::ResponseError(_) => "malformed response from the object store".to_owned(),
        _ => "unknown object storage failure".to_owned(),
    }
}

/// An in-memory `ObjectStore`, for tests only.
///
/// Keyed on `(Bucket, String)` rather than on the key alone, so a test that
/// writes to one bucket and reads from the other sees the miss instead of
/// silently passing - the same cross-bucket mistake the `Bucket` enum stops at
/// compile time, caught here for the paths where the bucket is chosen at
/// runtime.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryStore {
    objects: std::sync::Mutex<std::collections::HashMap<(Bucket, String), Vec<u8>>>,
}

#[cfg(test)]
impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn put(&self, bucket: Bucket, key: &str, body: Vec<u8>) -> Result<(), StorageError> {
        self.lock().insert((bucket, key.to_owned()), body);
        Ok(())
    }

    fn get(&self, bucket: Bucket, key: &str, limit: usize) -> Result<Vec<u8>, StorageError> {
        let objects = self.lock();
        let body = objects
            .get(&(bucket, key.to_owned()))
            .ok_or(StorageError::NotFound)?;

        if body.len() > limit {
            return Err(StorageError::TooLarge { limit });
        }

        Ok(body.clone())
    }

    fn delete(&self, bucket: Bucket, key: &str) -> Result<(), StorageError> {
        self.lock().remove(&(bucket, key.to_owned()));
        Ok(())
    }

    fn delete_prefix(&self, bucket: Bucket, prefix: &str) -> Result<(), StorageError> {
        self.lock()
            .retain(|(b, key), _| *b != bucket || !key.starts_with(prefix));
        Ok(())
    }

    /// Number of objects held, for assertions about what an operation left
    /// behind.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, bucket: Bucket, key: &str) -> bool {
        self.lock().contains_key(&(bucket, key.to_owned()))
    }

    /// The lock is only ever held for the body of one of the methods above,
    /// none of which can panic while holding it, so a poisoned mutex here
    /// would mean a bug elsewhere in the test. Unwrapping is the right
    /// response to that in test-only code.
    fn lock(
        &self,
    ) -> std::sync::MutexGuard<'_, std::collections::HashMap<(Bucket, String), Vec<u8>>> {
        self.objects.lock().expect("MemoryStore mutex poisoned")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::StorageConfig;

    fn config() -> StorageConfig {
        StorageConfig {
            endpoint: Some("http://garage:3900".to_owned()),
            region: "us-east-1".to_owned(),
            access_key_id: "key".to_owned(),
            secret_access_key: "secret".to_owned(),
            force_path_style: true,
            bucket_attachments: "pwnforge-attachments".to_owned(),
            bucket_avatars: "pwnforge-avatars".to_owned(),
            key_prefix: None,
            public_base_url: None,
        }
    }

    /// The key is a pure function of its three inputs, which is what lets a
    /// row hold only `avatar_version` and still name exactly one object.
    #[test]
    fn an_avatar_key_is_derived_from_kind_id_and_version() {
        let id = Uuid::parse_str("018f3b8e-0000-7000-8000-000000000001").unwrap();
        let version = Uuid::parse_str("018f3b8e-0000-7000-8000-000000000002").unwrap();

        assert_eq!(
            avatar_key(AvatarKind::User, id, version, AvatarSize::Large),
            "avatars/users/018f3b8e-0000-7000-8000-000000000001/018f3b8e-0000-7000-8000-000000000002-256.webp"
        );
        assert_eq!(
            avatar_key(AvatarKind::Team, id, version, AvatarSize::Small),
            "avatars/teams/018f3b8e-0000-7000-8000-000000000001/018f3b8e-0000-7000-8000-000000000002-64.webp"
        );
        assert_eq!(
            avatar_key(AvatarKind::Workspace, id, version, AvatarSize::Large),
            "avatars/workspaces/018f3b8e-0000-7000-8000-000000000001/018f3b8e-0000-7000-8000-000000000002-256.webp"
        );
    }

    /// The two renditions of one version must be distinct objects, and both
    /// must sit under the prefix used to delete that version.
    #[test]
    fn both_renditions_sit_under_the_version_prefix() {
        let id = Uuid::now_v7();
        let version = Uuid::now_v7();
        let prefix = avatar_version_prefix(AvatarKind::User, id, version);

        let large = avatar_key(AvatarKind::User, id, version, AvatarSize::Large);
        let small = avatar_key(AvatarKind::User, id, version, AvatarSize::Small);

        assert_ne!(large, small, "one key cannot hold two renditions");
        assert!(large.starts_with(&prefix), "{large} is not under {prefix}");
        assert!(small.starts_with(&prefix), "{small} is not under {prefix}");
    }

    /// The version prefix must not match a *different* version that happens to
    /// share a textual head. Uuids are fixed width so this holds structurally,
    /// but the `-` separator is what makes it hold rather than luck.
    #[test]
    fn a_version_prefix_does_not_reach_another_version() {
        let id = Uuid::now_v7();
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();

        let prefix = avatar_version_prefix(AvatarKind::User, id, a);
        let other = avatar_key(AvatarKind::User, id, b, AvatarSize::Large);

        assert!(!other.starts_with(&prefix), "{prefix} reached {other}");
    }

    /// Two entities of different kinds sharing an id must not share a key.
    /// Ids are `uuidv7()` per table, so a collision across tables is not
    /// something the database prevents.
    #[test]
    fn kinds_do_not_share_a_keyspace() {
        let id = Uuid::now_v7();
        let version = Uuid::now_v7();

        let keys = [
            avatar_key(AvatarKind::User, id, version, AvatarSize::Large),
            avatar_key(AvatarKind::Team, id, version, AvatarSize::Large),
            avatar_key(AvatarKind::Workspace, id, version, AvatarSize::Large),
        ];

        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                assert_ne!(a, b, "two kinds produced the same key");
            }
        }
    }

    /// Every version of an entity's avatar sits under the prefix used to purge
    /// it, or deleting the entity would leak objects.
    #[test]
    fn every_version_sits_under_the_purge_prefix() {
        let id = Uuid::now_v7();
        let prefix = avatar_prefix(AvatarKind::Team, id);

        for _ in 0..5 {
            let key = avatar_key(AvatarKind::Team, id, Uuid::now_v7(), AvatarSize::Large);
            assert!(key.starts_with(&prefix), "{key} is not covered by {prefix}");
        }
    }

    /// Attachment keys are built from ids, so renaming a workspace or a
    /// challenge cannot orphan an object.
    #[test]
    fn an_attachment_key_is_built_from_ids_only() {
        let workspace = Uuid::now_v7();
        let challenge = Uuid::now_v7();
        let attachment = Uuid::now_v7();

        let key = attachment_key(workspace, challenge, attachment);

        assert_eq!(key, format!("{workspace}/{challenge}/{attachment}"));
    }

    /// No CDN configured is the default, and the caller has to fall back to
    /// the API route rather than building a half-formed URL.
    #[test]
    fn no_public_base_url_means_no_public_url() {
        let config = config();
        assert!(avatar_public_url(&config, "avatars/users/a/b.webp").is_none());
    }

    #[test]
    fn a_public_url_joins_the_base_the_prefix_and_the_key() {
        let mut config = config();
        config.public_base_url = Some("https://cdn.example.test".to_owned());

        assert_eq!(
            avatar_public_url(&config, "avatars/users/a/b.webp").as_deref(),
            Some("https://cdn.example.test/avatars/users/a/b.webp")
        );

        config.key_prefix = Some("tenant-1".to_owned());

        assert_eq!(
            avatar_public_url(&config, "avatars/users/a/b.webp").as_deref(),
            Some("https://cdn.example.test/tenant-1/avatars/users/a/b.webp")
        );
    }

    /// `a/b` and `a/b/` are the same prefix, and an operator will write both.
    #[test]
    fn a_key_prefix_normalises_to_one_trailing_slash() {
        let mut config = config();

        for raw in ["tenant-1", "/tenant-1", "tenant-1/", "/tenant-1/"] {
            config.key_prefix = Some(raw.to_owned());
            assert_eq!(config.normalized_key_prefix(), "tenant-1/", "for {raw:?}");
        }

        config.key_prefix = None;
        assert_eq!(config.normalized_key_prefix(), "");
    }

    #[tokio::test]
    async fn the_memory_store_round_trips_an_object() {
        let store = ObjectStore::Memory(MemoryStore::new());

        store
            .put(
                Bucket::Avatars,
                "avatars/users/a/b.webp",
                vec![1, 2, 3],
                "image/webp",
            )
            .await
            .unwrap();

        assert_eq!(
            store
                .get(Bucket::Avatars, "avatars/users/a/b.webp", 1024)
                .await
                .unwrap(),
            vec![1, 2, 3]
        );
    }

    /// The bucket is part of the identity of an object, so the same key in the
    /// other bucket is a miss rather than a hit.
    #[tokio::test]
    async fn buckets_do_not_share_a_keyspace() {
        let store = ObjectStore::Memory(MemoryStore::new());

        store
            .put(Bucket::Avatars, "shared/key", vec![1], "image/webp")
            .await
            .unwrap();

        let err = store
            .get(Bucket::Attachments, "shared/key", 1024)
            .await
            .expect_err("the other bucket must not see it");

        assert!(matches!(err, StorageError::NotFound));
    }

    #[tokio::test]
    async fn a_missing_object_is_not_found() {
        let store = ObjectStore::Memory(MemoryStore::new());

        let err = store
            .get(Bucket::Avatars, "nope", 1024)
            .await
            .expect_err("a missing key must not read as an empty object");

        assert!(matches!(err, StorageError::NotFound));
    }

    /// Deleting something that is not there is the caller's intent already
    /// satisfied, not a failure. S3 behaves this way and so must the double.
    #[tokio::test]
    async fn deleting_a_missing_object_succeeds() {
        let store = ObjectStore::Memory(MemoryStore::new());
        assert!(store.delete(Bucket::Avatars, "nope").await.is_ok());
    }

    /// `delete_prefix` is what purges an entity's avatars, and it must not
    /// reach into the other bucket or past the prefix it was given.
    #[tokio::test]
    async fn delete_prefix_removes_only_the_matching_bucket_and_prefix() {
        let store = ObjectStore::Memory(MemoryStore::new());
        let id = Uuid::now_v7();
        let other = Uuid::now_v7();

        for _ in 0..3 {
            store
                .put(
                    Bucket::Avatars,
                    &avatar_key(AvatarKind::User, id, Uuid::now_v7(), AvatarSize::Large),
                    vec![1],
                    "image/webp",
                )
                .await
                .unwrap();
        }

        let keep_other_entity =
            avatar_key(AvatarKind::User, other, Uuid::now_v7(), AvatarSize::Large);
        let keep_other_kind = avatar_key(AvatarKind::Team, id, Uuid::now_v7(), AvatarSize::Large);
        store
            .put(Bucket::Avatars, &keep_other_entity, vec![2], "image/webp")
            .await
            .unwrap();
        store
            .put(Bucket::Avatars, &keep_other_kind, vec![3], "image/webp")
            .await
            .unwrap();
        store
            .put(
                Bucket::Attachments,
                &avatar_prefix(AvatarKind::User, id),
                vec![4],
                "application/octet-stream",
            )
            .await
            .unwrap();

        store
            .delete_prefix(Bucket::Avatars, &avatar_prefix(AvatarKind::User, id))
            .await
            .unwrap();

        let ObjectStore::Memory(memory) = &store else {
            unreachable!("constructed as Memory above")
        };

        assert_eq!(memory.len(), 3, "only the three versions must be gone");
        assert!(memory.contains(Bucket::Avatars, &keep_other_entity));
        assert!(memory.contains(Bucket::Avatars, &keep_other_kind));
    }

    /// The read limit is enforced by the store, not by its caller, so no
    /// caller can forget it.
    #[tokio::test]
    async fn an_oversized_object_is_refused_on_read() {
        let store = ObjectStore::Memory(MemoryStore::new());

        store
            .put(Bucket::Avatars, "big", vec![0; 2048], "image/webp")
            .await
            .unwrap();

        let err = store
            .get(Bucket::Avatars, "big", 1024)
            .await
            .expect_err("past the limit must be refused");

        assert!(matches!(err, StorageError::TooLarge { limit: 1024 }));
    }

    /// A minimal `ProvideErrorMetadata`, so the classifier can be tested
    /// without a provider on the other end.
    struct Meta {
        code: Option<&'static str>,
        message: Option<&'static str>,
    }

    impl ProvideErrorMetadata for Meta {
        fn meta(&self) -> &aws_sdk_s3::error::ErrorMetadata {
            unimplemented!("only code() and message() are used")
        }
        fn code(&self) -> Option<&str> {
            self.code
        }
        fn message(&self) -> Option<&str> {
            self.message
        }
    }

    /// The real Garage refusal, captured verbatim from a bucket whose
    /// `set-quotas --max-size` was exceeded. It arrives as `AccessDenied`,
    /// which is why the message has to be consulted at all.
    #[test]
    fn a_garage_quota_refusal_is_recognised() {
        let refusal = classify_refusal(&Meta {
            code: Some("AccessDenied"),
            message: Some(
                "Forbidden: Bucket size quota is reached, maximum total size of objects \
                 for this bucket: 20000000. The bucket is already 300000000 bytes, and \
                 this object would add 5000000 bytes.",
            ),
        });

        let Refusal::Quota(detail) = refusal else {
            panic!("a quota refusal must be classified as Quota");
        };

        assert!(
            detail.contains("quota"),
            "the detail must carry the provider's explanation, got {detail:?}"
        );
    }

    /// A provider that names the condition in the code itself. This branch is
    /// portable; the message-based one above is not.
    #[test]
    fn an_explicit_quota_code_is_recognised() {
        for code in ["QuotaExceeded", "XMinioAdminBucketQuotaExceeded"] {
            assert!(
                matches!(
                    classify_refusal(&Meta {
                        code: Some(code),
                        message: None
                    }),
                    Refusal::Quota(_)
                ),
                "{code} must be classified as Quota"
            );
        }
    }

    /// The one that matters most: an unrecognised 403 must fall back to a
    /// permission refusal, never to a full bucket.
    ///
    /// This is the safe direction. Reported as `Denied`, an operator looks at
    /// the credential's grants, which is where the problem is whenever the
    /// quota heuristic does not fire - including on AWS and R2, which have no
    /// per-bucket quota at all and can never produce that message.
    #[test]
    fn an_unattributable_denial_falls_back_to_access_denied() {
        let cases: [(&str, Option<&str>); 5] = [
            ("AccessDenied", Some("Access Denied.")),
            ("AccessDenied", None),
            ("AccessDenied", Some("")),
            // A provider that words its quota differently, or translates it:
            // the rule does not fire, and the fallback is the safe one.
            ("AccessDenied", Some("Depassement de la limite du bucket")),
            ("Forbidden", Some("not authorized to perform s3:PutObject")),
        ];

        for (code, message) in cases {
            assert!(
                matches!(
                    classify_refusal(&Meta {
                        code: Some(code),
                        message
                    }),
                    Refusal::Denied(_)
                ),
                "({code}, {message:?}) must fall back to Denied, never Quota"
            );
        }
    }

    /// Anything that is not a refusal stays opaque rather than being forced
    /// into one of the two buckets.
    #[test]
    fn a_non_refusal_is_not_classified_as_either() {
        for (code, message) in [
            ("NoSuchBucket", "The specified bucket does not exist"),
            ("InternalError", "We encountered an internal error"),
            ("SlowDown", "Please reduce your request rate"),
        ] {
            assert!(
                matches!(
                    classify_refusal(&Meta {
                        code: Some(code),
                        message: Some(message)
                    }),
                    Refusal::Other
                ),
                "{code} must be Other"
            );
        }
    }

    /// The word "quota" is only consulted under a denial code, so an unrelated
    /// failure that happens to mention it is not swept up.
    #[test]
    fn the_word_quota_alone_is_not_enough() {
        assert!(matches!(
            classify_refusal(&Meta {
                code: Some("InternalError"),
                message: Some("failed while computing the quota report"),
            }),
            Refusal::Other
        ));
    }

    /// The S3 client builds without reaching the network, and building it must
    /// not consult any AWS credential source: our own config is the only
    /// input. A regression here would show up as a boot-time hang against
    /// 169.254.169.254 rather than as a test failure, so it is pinned.
    #[test]
    fn the_s3_client_builds_from_our_config_alone() {
        // Safety: the process must not be reading these during the test. It is
        // not - nothing in this test touches `Config::from_env`.
        let store = S3Store::new(&config());

        assert_eq!(
            store.bucket_name(Bucket::Attachments),
            "pwnforge-attachments"
        );
        assert_eq!(store.bucket_name(Bucket::Avatars), "pwnforge-avatars");
        assert_eq!(store.key_prefix, "");
    }

    /// The prefix is applied by the store, so no caller has to remember it and
    /// no caller can apply it twice.
    #[test]
    fn the_store_applies_the_key_prefix() {
        let mut config = config();
        config.key_prefix = Some("tenant-1".to_owned());

        let store = S3Store::new(&config);

        assert_eq!(
            store.prefixed("avatars/users/a/b.webp"),
            "tenant-1/avatars/users/a/b.webp"
        );
    }
}
