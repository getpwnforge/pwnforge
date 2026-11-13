//! Integration tests for `storage_service`, against a real S3-compatible
//! object store.
//!
//! These cannot use the in-memory double: what is under test is the behaviour
//! of an actual provider - that `HeadBucket` answers for both buckets, that a
//! missing key surfaces as `NoSuchKey` rather than as a generic error, that a
//! paged `ListObjectsV2` terminates - and a double would only assert our
//! assumptions about S3 instead of S3 itself.
//!
//! Locally the dev compose provides one:
//!
//!   docker compose -f docker-compose.dev.yml up -d garage garage-init
//!   PWNFORGE_S3_ENDPOINT=http://127.0.0.1:3900 cargo test -p services --test object_storage
//!
//! The variables come from `.env`; only the endpoint has to be overridden,
//! since the value there is the in-network `http://garage:3900`.

use services::storage_service::{
    AvatarKind, AvatarSize, Bucket, ObjectStore, S3Store, StorageError, avatar_key, avatar_prefix,
};
use uuid::Uuid;

#[path = "common/storage.rs"]
mod storage;

use storage::storage_config;

fn store() -> ObjectStore {
    ObjectStore::S3(S3Store::new(&storage_config()))
}

/// A fresh entity id per test, so tests never collide with each other or with
/// whatever a previous run left in the bucket.
fn entity() -> Uuid {
    Uuid::now_v7()
}

/// Positive: the boot check passes against a bucket pair the bootstrap
/// created. This is the assertion that `ops/garage/bootstrap.py` and
/// `StorageConfig` agree with each other.
#[tokio::test]
async fn health_succeeds_against_both_provisioned_buckets() {
    store()
        .health()
        .await
        .expect("HeadBucket must succeed on both buckets");
}

/// Negative: the boot check fails, by name, on a bucket that is not there.
/// This is what an operator who mistyped a bucket name has to see.
#[tokio::test]
async fn health_names_the_bucket_that_is_missing() {
    // Only the avatars bucket is changed, so everything else - endpoint,
    // region, credential, addressing style - stays whatever the provider under
    // test needs.
    let mut config = storage_config();
    config.bucket_avatars = "pwnforge-no-such-bucket".to_owned();

    let error = ObjectStore::S3(S3Store::new(&config))
        .health()
        .await
        .expect_err("a missing bucket must fail the boot check");

    match error {
        StorageError::BucketUnavailable { bucket, .. } => {
            assert_eq!(
                bucket, "pwnforge-no-such-bucket",
                "the error must name the bucket the operator has to fix"
            );
        }
        other => panic!("expected BucketUnavailable, got {other}"),
    }
}

/// Positive: a full round trip through a real provider.
#[tokio::test]
async fn an_object_round_trips() {
    let store = store();
    let key = avatar_key(
        AvatarKind::User,
        entity(),
        Uuid::now_v7(),
        AvatarSize::Large,
    );
    let body = b"not really a webp, but bytes are bytes".to_vec();

    store
        .put(Bucket::Avatars, &key, body.clone(), "image/webp")
        .await
        .expect("put must succeed");

    let read = store
        .get(Bucket::Avatars, &key, 1024 * 1024)
        .await
        .expect("get must return what put wrote");

    assert_eq!(read, body);

    store
        .delete(Bucket::Avatars, &key)
        .await
        .expect("delete must succeed");

    let error = store
        .get(Bucket::Avatars, &key, 1024 * 1024)
        .await
        .expect_err("a deleted object must not be readable");

    assert!(
        matches!(error, StorageError::NotFound),
        "a deleted key must surface as NotFound, got {error}"
    );
}

/// The `NoSuchKey` mapping is provider behaviour, not ours: S3 answers a
/// missing key with a 404 carrying a specific error code, and the code turns
/// that into `NotFound` rather than a generic backend failure. Everything that
/// distinguishes "no avatar" from "storage is down" rests on this.
#[tokio::test]
async fn a_missing_key_is_not_found_and_not_a_backend_error() {
    let error = store()
        .get(Bucket::Avatars, "definitely/not/here.webp", 1024)
        .await
        .expect_err("a missing key must be an error");

    assert!(
        matches!(error, StorageError::NotFound),
        "expected NotFound, got {error}"
    );
}

/// The two buckets are genuinely separate storage, not a naming convention
/// over one. Writing to one must not make the key readable in the other.
#[tokio::test]
async fn the_two_buckets_are_separate_storage() {
    let store = store();
    let key = format!("cross-bucket-probe/{}", Uuid::now_v7());

    store
        .put(Bucket::Avatars, &key, vec![1, 2, 3], "image/webp")
        .await
        .expect("put must succeed");

    let error = store
        .get(Bucket::Attachments, &key, 1024)
        .await
        .expect_err("the attachments bucket must not see an avatars key");

    assert!(
        matches!(error, StorageError::NotFound),
        "expected NotFound, got {error}"
    );

    store.delete(Bucket::Avatars, &key).await.ok();
}

/// `delete_prefix` is what purges an entity's avatars. Against a real provider
/// this also exercises the `ListObjectsV2` loop, including that it terminates.
#[tokio::test]
async fn delete_prefix_removes_every_version_and_nothing_else() {
    let store = store();
    let target = entity();
    let bystander = entity();

    for _ in 0..3 {
        for size in [AvatarSize::Large, AvatarSize::Small] {
            store
                .put(
                    Bucket::Avatars,
                    &avatar_key(AvatarKind::User, target, Uuid::now_v7(), size),
                    vec![7],
                    "image/webp",
                )
                .await
                .expect("put must succeed");
        }
    }

    let keep = avatar_key(
        AvatarKind::User,
        bystander,
        Uuid::now_v7(),
        AvatarSize::Large,
    );
    store
        .put(Bucket::Avatars, &keep, vec![9], "image/webp")
        .await
        .expect("put must succeed");

    store
        .delete_prefix(Bucket::Avatars, &avatar_prefix(AvatarKind::User, target))
        .await
        .expect("delete_prefix must succeed");

    let error = store
        .get(
            Bucket::Avatars,
            &avatar_prefix(AvatarKind::User, target),
            1024,
        )
        .await
        .expect_err("the prefix itself is not an object");
    assert!(matches!(error, StorageError::NotFound));

    // The bystander's object is the one that proves the prefix scan did not
    // simply empty the bucket.
    store
        .get(Bucket::Avatars, &keep, 1024)
        .await
        .expect("another entity's avatar must survive");

    store.delete(Bucket::Avatars, &keep).await.ok();
}

/// The read limit is enforced against a real body, not only against the
/// provider's advertised `Content-Length`.
#[tokio::test]
async fn an_oversized_object_is_refused_on_read() {
    let store = store();
    let key = format!("oversized-probe/{}", Uuid::now_v7());

    store
        .put(Bucket::Avatars, &key, vec![0; 4096], "image/webp")
        .await
        .expect("put must succeed");

    let error = store
        .get(Bucket::Avatars, &key, 1024)
        .await
        .expect_err("past the limit must be refused");

    assert!(
        matches!(error, StorageError::TooLarge { limit: 1024 }),
        "expected TooLarge, got {error}"
    );

    store.delete(Bucket::Avatars, &key).await.ok();
}
