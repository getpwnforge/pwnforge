# 0002 - Replacing MinIO: Garage vs SeaweedFS

- **Status**: **decided, Garage**
- **Date**: evaluated 2026-09-22, decided 2026-09-23
- **Scope**: `docker-compose*.yml`, `ops/garage/` (replaces `ops/minio/`),
  `.env.example`, `.gitignore`. **Not** the Rust code (see section 1).

## Decision

Garage replaces MinIO as the bundled object store.

**What settles it is not the memory footprint, it is the SeaweedFS
divergence.** A `GetObject` that answers `200` with an empty body instead of
`404` is filesystem semantics surfacing through the S3 API. Our code is not
reached by it today, since our keys never end in `/`, but what it documents is
the quality of the S3 implementation itself. We ask third parties to substitute
their own object store for this component, so the layer has to behave like S3
rather than like a filer in disguise.

The second argument is rotation, which stays a bounded, scripted procedure on
Garage, whereas SeaweedFS turns it into a restart of the storage server because
the credential lives in a configuration file read at boot. **See the correction
in section 2, however: "bounded" does not mean "re-run the same bootstrap".**

The memory footprint (14 MiB against 570) and the AGPLv3 licence alignment
support the choice without being the reason for it.

## Why this note exists

MinIO Community Edition is end of life:

| Date | Event |
| --- | --- |
| 05/2025 | Console removed from CE |
| 10/2025 | Binary distribution stopped |
| 12/2025 | Moved to maintenance mode |
| 13/02/2026 | Repository archived. No further upstream security fixes, and CVEs are already published against the last community release |
| 09/2026 | Images removed from Docker Hub |

The last point was hit directly: `minio/minio:latest`, which the Compose stack
used, no longer pulls at all. The pin that replaced it,
`quay.io/minio/minio:RELEASE.2025-04-22T22-12-26Z`, was therefore not a
conservative choice. It was seventeen months without a patch on an archived
project, kept only because it is the last release that still ships the console,
which the production stack disables anyway. For a security product shipped
self-hosted by default, that is not a position to settle into.

Two candidates were evaluated as the **Compose default**: Garage (Rust,
AGPLv3, Deuxfleurs) and SeaweedFS (Go, Apache 2.0).

## Method

Everything below is measured, not cited. Both candidates were started as
single-node containers and **our own test suite**
(`crates/services/tests/object_storage.rs`) was run against each, with nothing
recompiled differently: the same test binary, driven only by the
`PWNFORGE_S3_*` variables.

That is the only honest way to answer "does the Rust code have to change".

## 1. The Rust code does not change, verified

The code issues exactly five S3 verbs and nothing else:

```
1 put_object    1 get_object    2 delete_object
1 head_bucket   1 list_objects_v2
```

No versioning call, no bucket policy, no lifecycle, no object lock, and no
administration API. No MinIO-specific call.

**No multipart call is issued today.** `put_object` with an in-memory body does
a single PUT, which S3 allows up to 5 GB, so a 50 MB attachment goes through
without multipart. Multipart stays granted in the application permissions
because it will be needed once large uploads are streamed rather than buffered,
but it is **not** a selection criterion today: there is no code behind it. See
ADR 0001 section 4, which turns that observation into a design constraint.

Suite results:

| | MinIO (ref.) | Garage v2.4.1 | SeaweedFS 4.47 |
| --- | --- | --- | --- |
| `object_storage` | **7/7** | **7/7** | **6/7** |

One test had to be fixed along the way, and it was **a defect in our test**,
not in the code: it hardcoded `us-east-1`. Garage signs against the region
`garage` and R2 against `auto`, so that test would always have passed on MinIO
and failed everywhere else. It now reads the region from the environment like
everything else. Not one line of `storage_service.rs` changed.

### The SeaweedFS divergence

The failing test is `delete_prefix_removes_every_version_and_nothing_else`, and
the cause is real: a `GetObject` on a key ending in `/` that corresponds to an
**existing directory** in the filer returns `200` with an empty body, instead
of the `404 NoSuchKey` that MinIO and Garage return. A prefix that does not
exist does return `404`.

That is SeaweedFS's filesystem semantics showing through the S3 API. Our code
is not reached in practice, since `avatar_key` never produces a key ending in
`/`, but it is the kind of divergence that turns "the avatar is absent" into
"the avatar is a zero-byte image" if a key ever coincides with a prefix. On
Garage and MinIO the absent/empty distinction is clean.

## 2. What becomes of the bootstrap

This is where the gap is widest, and the current script disappears either way:
`mc admin policy` is a MinIO administration API, not S3.

### Garage, corrected after implementation

The evaluation described this section from CLI experiments. Implementation
invalidated two of its claims. They are corrected here, and they are the only
two things in this whole document that the work disproved.

**Correction 1: the Garage image has no shell, so no shell script.**
`dxflrs/garage` is built `FROM scratch`: it contains the `garage` binary and
nothing else. An `sh` script cannot run in it, and the CLI cannot run anywhere
else, because remote administration requires
`--rpc-host <full-node-id>@host:port` and the node id is only knowable once the
node has started.

The bootstrap therefore talks to Garage's **admin HTTP API**, from an official
`python:3.13-alpine` image, using nothing but the standard library. No image to
build, which would have meant a `build:` in the production stack that uses only
prebuilt images plus a change to `docker-build.yml`, and no packages installed
at container start. See `ops/garage/bootstrap.py`.

**Correction 2: rotation is NOT "same id, new secret".** This is the most
consequential error in the evaluation, and it touches the second argument of
the decision. Garage:

- refuses to change an existing key's secret (`UpdateKey` accepts the field and
  silently ignores it);
- refuses to reuse an access key id, **permanently, including after the key is
  deleted**:

```
Error: ImportKey returned KeyAlreadyExists (409): Key GK... already exists in
data store. Even if it is deleted, we can't let you create a new key with the
same ID. Sorry.
```

Rotating therefore means issuing a **new key**: a new id *and* a new secret in
`.env`, `docker compose run --rm garage-init` to import it and grant it both
buckets, recreating the backend, and only then deleting the old key by hand.
That last step is not automated: the script has no way to know whether the
backend has picked up the new key, and deleting the one still in use would take
the instance down.

It is still a bounded, scripted procedure, better than the storage-server
restart SeaweedFS imposes, but it is not "re-run the same bootstrap", and
`.env.example` now says so explicitly.

**The old key stays valid until step 4 runs**, which matters most in the case
that makes anyone rotate in the first place. A rotation prompted by a
compromise leaves the compromised credential live for as long as the operator
forgets the last step, and the instance keeps working perfectly throughout, so
nothing prompts them.

Documentation is not enough for that. Every bootstrap run now lists the keys
that can reach each bucket and warns when it finds more than one, naming the
unexpected key and printing the `garage key delete` command for it. The
operator sees it on the run that imported the new key, which is the moment they
are already looking at the output. It warns rather than fails, because two keys
is a legitimate mid-rotation state and refusing to boot would make the safe
path the one that breaks.

**Correction 3: `-c` is not a quota.** The capacity parameter is a **relative
weight in the partition ring**, not a storage limit. Measured: a node
advertising `100M` accepted 300 MB across six objects and read every one of
them back without error. On a single node the value has no practical effect at
all, since one node is always 100% of the ring.

What actually bounds storage is the filesystem and, below that, a **per-bucket
quota**: `garage bucket set-quotas <bucket> --max-size 50G`. That one is a real
refusal; see section 5.

**What holds from the evaluation:** the admin/application separation holds and
is simpler to express (`Can create buckets: false` is the default, verified at
import time, since the `ImportKey` response carries
`"permissions": {"createBucket": false}`), permissions are granted bucket by
bucket with no `owner`, and `layout assign` is one extra step specific to
Garage.

**Idempotence, verified against the real Compose stack.** Three consecutive
runs, `exit 0` each time, one of them after restarting the instance with data
already written. The three test objects survived and the layout was left
untouched. The traps, all handled in `bootstrap.py`:

| Call | Second run | Handling |
| --- | --- | --- |
| `ApplyClusterLayout` | 500 "Invalid new layout version" | version read from the cluster, and the step skipped entirely once the node has a role |
| `CreateBucket` | 409 | tolerated, the id is re-read through `GetBucketInfo` |
| `ImportKey` | 409 | existence is tested first, the key is left as it is |
| `AllowBucketKey` | 200 | replayed unconditionally |

### SeaweedFS

No administration CLI for identities: they live in a JSON file (`-s3.config`)
mounted into the container, with actions scoped per bucket:

```json
{"identities":[{"name":"pwnforge-app",
  "credentials":[{"accessKey":"...","secretKey":"..."}],
  "actions":["Read:pwnforge-avatars","Write:pwnforge-avatars","List:pwnforge-avatars",
             "Read:pwnforge-attachments","Write:pwnforge-attachments","List:pwnforge-attachments"]}]}
```

The separation holds too, verified: `Access Denied` on `mb`, on `rb`, and on
any other bucket. But the model changes in nature. The application credential
sits in a configuration file read at server startup rather than being created
by an administration call. The concrete consequences:

- **Rotation becomes a restart of the storage server**, not a one-shot command.
- The application secret ends up in a mounted file, in addition to `.env`.
- There is no distinct notion of "root" any more: an `admin` identity in the
  same file creates the buckets. The separation exists, but both credentials
  sit side by side in the same artifact.

## 3. Single node and memory footprint

Measured at rest, same machine, container alone:

| | Image | RSS at rest |
| --- | --- | --- |
| **Garage** v2.4.1 | 99 MB | **14.2 MiB** |
| MinIO (ref.) | 250 MB | 267 MiB |
| **SeaweedFS** 4.47 | 729 MB | **570 MiB** |

Garage is **40x lighter than SeaweedFS** and 19x lighter than MinIO. For a
Compose default aimed at self-hosters running the stack on a small VPS next to
Postgres, Redis, the backend and nginx, the gap is not cosmetic: the `backend`
service is capped at 512 MB and SeaweedFS uses more than that while idle.

**Garage on a single node when it targets 3 or more:** `replication_factor = 1`
is a first-class value, not a workaround. The node reports `HEALTHY` with no
degraded-mode warning, and all seven tests pass. The cost is the
`layout assign` step at provisioning, which only makes sense in a distributed
model but is mandatory even with one node. Garage does not pretend to be
replicated: at factor 1 there is simply no redundancy, which is exactly the
situation with single-node MinIO today.

Worth noting: Garage returns `501 Not Implemented` for `ListObjectVersions` and
`GetObjectLockConfiguration`. Those calls came from `mc` during testing, never
from our code, and they are precisely the kind of API that ADR 0001 rules out
in order to stay portable.

## 4. Published images, cadence, security responsiveness

| | Garage | SeaweedFS |
| --- | --- | --- |
| Images | `dxflrs/garage` (Docker Hub), `vX.Y.Z` tags plus one per commit | `chrislusf/seaweedfs` (Docker Hub), cosign-signed tags |
| Latest release | **v2.4.1, 08/09/2026** | **4.47, 14/09/2026** |
| Cadence | v2.4.0 (06/09), v2.3.0 (16/04), v2.2.0 (24/01), v2.1.0 (15/09/2025). Roughly quarterly minors, fast patches | 4.47 (14/09), 4.46 (08/09), 4.45 (31/08), 4.44 (22/08). Weekly to fortnightly |
| Licence | AGPLv3, **the same as PwnForge** | Apache 2.0 |
| Governance | Deuxfleurs, an association, collective development | Very strongly centred on one main maintainer |

SeaweedFS releases far more often, which cuts both ways: fixes arrive quickly,
and the surface changes quickly. Garage publishes less often but its latest
release is two weeks old.

Garage's AGPLv3 alignment is not a technical argument, but it is coherent:
bundling a component under the same licence as the product avoids having to
explain two regimes to self-hosters.

## 5. Capacity, quota, and what the backend says about it

`PWNFORGE_GARAGE_CAPACITY` exists and defaults to `100G`, but **it is not the
safeguard it looks like**: it is a ring weight with no effect on a single node
(section 2, correction 3). It is exposed anyway, because it becomes meaningful
as soon as a second node appears and a hardcoded value would then be wrong for
everyone.

The real ceiling is the per-bucket quota, and it produces a real refusal.
Measured, against a bucket with `--max-size 20M` already holding 300 MB:

```
HTTP 403, code AccessDenied
message: "Forbidden: Bucket size quota is reached, maximum total size of
          objects for this bucket: 20000000. The bucket is already 300000000
          bytes, and this object would add 5000000 bytes."
```

The code is `AccessDenied`, because S3 has no quota concept of its own, so
every implementation that adds one reuses an existing code. The consequence,
before it was fixed: our layer rendered that as
`Backend("unhandled error (AccessDenied)")`, mapped to **503 service
unavailable**. A full bucket therefore read as an outage, under a code
suggesting a permissions bug in our own code. That is exactly the opaque case
worth handling.

It is fixed, and the fix is deliberately lopsided. `classify_refusal` decides
in a fixed order:

1. A code naming a quota outright (`QuotaExceeded`,
   `XMinioAdminBucketQuotaExceeded`) is a quota. Portable.
2. A 403 whose message mentions a quota is a quota. **Best effort, and
   Garage-specific.**
3. Any other 403 is `StorageError::AccessDenied`.
4. Anything else stays an opaque `Backend` failure.

**Step 2 is the only provider-specific rule in the storage layer**, and it is
worth naming as such, because ADR 0001 exists partly to keep those out. S3 has
no quota concept, so Garage smuggles one inside `AccessDenied` with an
explanatory sentence. **AWS and R2 have no per-bucket quota at all and will
never produce that message.** Matching on prose is a Garage-shaped convenience,
not a portable contract, and it will stop working if the wording changes
upstream or is ever translated.

That is tolerable only because of the direction it can fail in. The rule can
only ever *upgrade* a refusal from "denied" to "full", never the reverse. When
it does not fire, the caller gets `AccessDenied`, which sends the operator to
look at the credential's grants. That is the right place on every provider that
has no quotas, and a harmless detour on Garage. A misfire the other way would
tell an operator to raise a quota that does not exist while a missing
`PutObject` grant was the real problem.

`AccessDenied` maps to **500** with a log line naming the likely cause: the
caller did nothing wrong and can do nothing about it, and the store is
answering fine, so neither 403 nor 503 would be honest. `QuotaExceeded` maps to
**507 Insufficient Storage** with the code `storage_quota_exceeded`: 507 rather
than 503 because retrying will never help, someone has to raise the quota or
delete objects.

Five tests cover the classifier. The one that matters is
`an_unattributable_denial_falls_back_to_access_denied`, which pins the default:
a bare `AccessDenied`, an empty message, a differently worded quota message,
and an AWS-style `not authorized to perform s3:PutObject` all have to come out
as `Denied`, never as a full bucket.

## 6. What the switch left aside

- **The dev stack loses the console.** Garage has no UI, and MinIO's was the
  only reason to stay on a 2025 release. Local inspection goes through the CLI
  or any S3 client, documented under "Inspecting object storage" in
  [CONTRIBUTING.md](../../CONTRIBUTING.md).
- **`ops/garage/garage.toml` is a new mandatory file**, gitignored, to be
  created from `garage.toml.example` before the first start. That is one more
  installation step than MinIO needed, imposed by the fact that the RPC secret
  and the admin token must live neither in `.env` nor in any application
  container's environment.
- **Nothing was migrated.** No data existed in the development MinIO and no
  instance is deployed. Switching a production instance would need a
  bucket-to-bucket copy (`rclone`, `mc mirror`) which is not written here
  because it has no subject.
- **CI needed an object store.** The `object_storage` and `avatars` tests talk
  to a real one, and the backend job had only Postgres and Redis, so it would
  have gone red on the first pull request. Garage cannot be a `services:`
  container, because services start before checkout and there would be no
  config file to mount, so it is started by a step after checkout and
  provisioned with the same `ops/garage/bootstrap.py` the Compose stack uses.
  Verified by running that step verbatim and the full suite against it.

## 7. Reading

Everything measured pointed at **Garage**, and the decision followed:

- 7/7 on our suite against 6/7, and the SeaweedFS divergence is filesystem
  semantics surfacing through S3. This is the deciding factor, because we ask
  third parties to substitute their own implementation.
- An admin/application separation that is simpler to express *and* whose "no
  bucket creation" is the default.
- A bounded, scripted rotation, though more constraining than expected
  (section 2).
- 14 MiB against 570 MiB, for a default aimed at modest machines.
- The same licence as the product.

What Garage costs: a `layout assign` step specific to it, a non-standard
signing region (`garage`), a mandatory configuration file, no console, no
versioning and no object lock (which we have no use for, and which ADR 0001
already forbids using), and a rotation that changes the access key id.

What would have argued for SeaweedFS: a higher patch cadence, a more permissive
licence, and a broader ecosystem. The price was a footprint incompatible with a
small host, a default configuration that fails opaquely (a 120 MB multipart
upload returns "We encountered an internal error" until `-volume.max` is
raised), and the approximate S3 implementation that settled the decision.

## 8. Noted for later

Garage supports **presigned URLs** and **static bucket serving**. The day
`PWNFORGE_S3_PUBLIC_BASE_URL` is genuinely used, the path exists with no extra
component to deploy. This is not an invitation to use it: ADR 0001 section 3
explains why nothing is served presigned, and that does not change. It is
recorded because the opposite, discovering that an extra nginx is needed to
front a CDN, would have been an unpleasant surprise.
