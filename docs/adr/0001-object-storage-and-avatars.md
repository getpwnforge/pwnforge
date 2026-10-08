# 0001 - Object storage and avatars

- **Status**: accepted
- **Date**: 2026-09-22
- **Scope**: `services::storage_service`, `services::avatar_service`,
  `api::routes::avatars`, `ops/garage/bootstrap.py`

## Context

PwnForge stores two families of files: challenge attachments (Phase 4, not yet
implemented) and avatars for users, teams and workspaces. S3 is the only
runtime backend. There is no local filesystem mode, and therefore no
`PWNFORGE_STORAGE_BACKEND` variable. Garage is bundled in the Compose stack by
default, but AWS S3, Cloudflare R2 and Scaleway must all work without forking
anything.

This document records the decisions and constraints that cannot be read off the
code.

## 1. Keys are derived from identifiers, never from slugs

Attachment key scheme:

```
{workspace_id}/{challenge_id}/{attachment_id}
```

Avatar key scheme:

```
avatars/{users|teams|workspaces}/{id}/{version}-{256|64}.webp
```

Slugs are mutable and S3 has no rename operation. A key built from
`{team_slug}/{ws_slug}/{challenge_slug}` becomes wrong the moment a workspace
is renamed, and the objects written under the old slug are then named by
nothing: never read again, never deleted, and billed to someone with no way to
find out why. An identifier does not change, so a key derived from one stays
correct for the lifetime of the object.

The database-side corollary: no row stores a key or a URL. The table carries
`avatar_version` (a nullable uuid) and nothing else, and the key is recomputed
by a pure function from `(kind, id, version)`. A column holding a key would let
any write anywhere in the application point a row at an arbitrary object in the
bucket, which is a far larger surface than the cache-busting convenience it
would buy.

The `-{256|64}` suffix is the one departure from the scheme as originally
specified. An upload produces two renditions and one key cannot hold two
objects, so the size has to appear somewhere. As a suffix it keeps both
renditions under the same version prefix, which is what lets the previous
version be deleted with a single prefix scan.

There is **no** data migration. No attachment exists today and no slug-shaped
key was ever written. The scheme is set down now so that the bucket the
bootstrap provisions has a documented keyspace, and so the slug-based scheme is
not reinvented when the feature lands.

## 2. Avatars follow a different regime from attachments

An avatar is never stored as it was received. The pipeline is:

1. **Strict magic-byte allowlist** (`infer`), before any decoding: PNG, JPEG,
   WebP. The request's `Content-Type` is ignored, because the uploader chooses
   it.
2. **SVG is refused explicitly**, with its own error. An SVG is not a raster
   image but a document: it can carry `<script>`, a remote `xlink:href`, and a
   `<foreignObject>` full of HTML. Served inline from our own origin, that is
   stored XSS.
3. **Bounded decoding**, inside a `spawn_blocking` with a timeout, and with
   `image::Limits` configured: 8000x8000 maximum, and an allocation ceiling of
   exactly one RGBA buffer at those dimensions. A decompression bomb is a few
   kilobytes on the wire and several gigabytes in memory; the limits are handed
   to the decoder before `decode()`, so it refuses while reading the header
   instead of allocating first.
4. **Resize to 256px and 64px, re-encode as WebP.** Only the re-encoded outputs
   reach the bucket. The original is never stored.

The `image` crate is compiled with `default-features = false` and exactly the
three formats on the allowlist. The other decoders are not merely refused at
runtime: they are not linked into the binary.

It is the re-encoding, and nothing else, that makes
`Content-Disposition: inline` acceptable here. A polyglot file that is
simultaneously a valid JPEG and a valid HTML document does not survive a round
trip through a pixel buffer, and neither does EXIF, a trailing ZIP archive, or
an embedded colour profile. Attachments are the opposite case, arbitrary
operator-supplied files, and stay on `application/octet-stream` plus
`attachment`. **The avatar regime must not be taken as a precedent for them.**

A decoder panic on hostile bytes produces a clean HTTP 500: the blocking task
is replaced by the pool, and it takes down neither the worker nor the process.

### `avatar_url` was dropped from all three tables

`users`, `teams` and `workspaces` each carried an `avatar_url` column,
described in all three migrations as "the URL of an uploaded image". That is
precisely a row naming an object in the bucket, the pattern `avatar_version`
replaces: a column holding a key or a URL lets any write that reaches it point
the row at an arbitrary object, whereas a version uuid can only ever name
objects belonging to its own entity.

It was not an external-avatar column. There is no OAuth anywhere in the
codebase, no provider table and no identity linking, so no GitHub or Discord
picture ever passed through it. Dropping it lost nothing: no code path ever
wrote it, its four reads copied it straight into a response DTO, and the
frontend does not reference the field. Every row was `NULL`.

If external avatars arrive one day through OAuth, the column comes back, but as
a distinct thing: a third-party URL is not a key into our bucket, and the two
would coexist under a documented precedence rule, uploaded avatar first and
external avatar as fallback. Two questions will then have to be settled, and
they are not settled here:

- **Rendering a third-party URL in the browser leaks the visitor's IP** to the
  provider, on every render, for every member shown in a list. For a security
  platform that is a passive and silent metadata leak.
- The remedy is to **fetch the image at first login** and treat it as an
  ordinary upload: same allowlist, same re-encoding, same bucket. It costs one
  outbound request at signup and turns an external avatar into an internal one,
  which also removes the precedence problem. It looks like the right option,
  but it is not decided.

## 3. Nothing is served by a presigned URL

Every file goes through an Axum endpoint that checks permission on **every**
request. Garage stays on the Compose `internal` network and publishes no port
in production.

A presigned URL is a bearer token with a lifetime of its own: once issued, it
cannot be revoked when someone leaves a team. Access control would then revert
to "who held a link at the moment it was issued" instead of "who is allowed
now". The endpoint costs one extra round trip, which is the price of an
authorization that stays true.

Garage, the bundled object store since ADR 0002, can do presigned URLs and can
serve a bucket statically. That is not an invitation: the rule above does not
change. It is recorded because it means that the day
`PWNFORGE_S3_PUBLIC_BASE_URL` is genuinely used, for avatars and only for
avatars, the path exists with no extra component to deploy.

The 404/403 rule from [docs/security-invariants.md](../security-invariants.md#a-404-hides-existence-a-403-admits-it)
applies: a non-member gets `404`, because a `403` would confirm the team or
workspace exists, and a member whose role is insufficient gets `403`.

## 4. Design constraint: attachments must be streamed

**Recorded now, before the code exists, because this is the only moment at
which it costs nothing.**

`ObjectStore::put` takes a `Vec<u8>` and `aws_sdk_s3::put_object` turns it into
a single PUT. S3 accepts a single PUT up to 5 GB, so "a 50 MB attachment goes
through without multipart" is accurate, and that is exactly the problem: a
single PUT means the whole body in memory.

The arithmetic is immediate. The `backend` service is capped at 512 MB in the
Compose stack. The request body limit is **per request, not cumulative**: ten
concurrent 50 MB uploads are 500 MB of buffers, plus the rest of the process. A
single authenticated account is enough to cause an OOM, with no exploit
involved, simply by opening requests in parallel.

Exposure is low today: only avatars exist, capped at 5 MB, and a dedicated rate
limit bounds their frequency. But the Phase 4 attachment path **will inherit
this buffered `put_object`** unless something prevents it, because it is the
path that already exists and it will appear to work.

So, as a constraint and not a TODO:

1. **Attachments must never be buffered in memory.** Their upload goes through
   a multipart, or through a body streamed directly to S3 (a `ByteStream` fed
   from the request stream). `ObjectStore::put` in its current shape is not
   usable for them: they need a separate entry point, not a widening of this
   one.
2. **The attachment upload route must carry a concurrency cap of its own**, not
   just a rate limit. A rate limit bounds frequency, not the number of
   simultaneous transfers, and it is the simultaneous ones that fill memory.
   The `GlobalConcurrencyLimitLayer` in `app.rs` is sized against the database
   pool and does not fit: an upload holds no connection.
3. **Reads have the same problem.** `ObjectStore::get` collects the whole body
   in memory, bounded by an explicit `limit` precisely so that nobody uses it
   without thinking about it. Serving an attachment will mean handing the
   `ByteStream` to the response body rather than collecting it.

Nothing changes for avatars: 5 MB buffered, with a body limit at the same value
and a dedicated rate limit, is acceptable and stays.

## 5. Accepted limitations

**The two buckets share a single application credential.** Separating
`pwnforge-attachments` from `pwnforge-avatars` buys clarity of policy and
lifecycle: a retention rule or a bucket policy can target one without reasoning
about the other. It does **not** buy blast-radius isolation, since a
compromised backend reaches both. The `Bucket` type prevents bucket confusion
at compile time; it prevents nothing against an attacker running code. Two
separate credentials would change that, at the cost of two sets of variables
and twice the rotation work. That is not the trade-off taken today.

**The avatar endpoint answers `Cache-Control: private, max-age=31536000,
immutable`.** `immutable` is accurate, because the URL names a version and a
new upload produces a new URL.

`private` rather than `public`, deliberately. A shared cache holding the bytes
would reduce the permission check to a guard on *discovering* the URL, which is
exactly the URL-as-capability model rejected when presigned URLs were ruled
out, reintroduced one header at a time. The browser cache behaves identically
under `private`, so the only thing given up is a shared cache we do not have.

The day a CDN is put in front of avatars, `public` becomes an explicit choice
to make **there**, and only for entities whose avatars are public anyway.
Setting `PWNFORGE_S3_PUBLIC_BASE_URL` is already that decision in another form:
a CDN then serves avatars with no permission check at all. Neither extends to
attachments, for which the code has no way to build a public URL
(`avatar_public_url` takes no `Bucket` parameter).

**The render timeout bounds the request, not the CPU.** A `spawn_blocking` task
cannot be cancelled: a pathological decode keeps running on the pool after the
client has been answered. What actually bounds the work is the decoder limits,
and what bounds the frequency is the dedicated rate limit on the upload route.
The timeout only guarantees that no client is left hanging.

**The migration's `down()` does not touch the bucket.** It drops the
`avatar_version` column; the objects remain. That is deliberate: a schema
migration has no credential for object storage, and a `down()` that deleted
data in a second system would be worse than a few orphaned objects.
`avatar_service::remove` collects them by prefix the next time an avatar is
cleared.

## Alternatives rejected

- **An `ObjectStore` trait.** There is one runtime implementation and there is
  not going to be a second. A trait would buy nothing but a dyn-safety
  constraint; an enum with inherent `async fn` keeps the futures concrete and
  `Send` with no attribute macro.
- **`aws-config` and its credential chain.** It falls back to EC2 IMDS and to
  the container credential endpoint when our variables are absent, so a
  misconfigured instance would issue requests to link-local addresses at boot
  instead of failing. Credentials come from our environment or the process does
  not start.
- **Signing SigV4 ourselves** (or `rusty-s3`). Writing your own request signing
  in a security product is not an acceptable trade for removing one dependency.

There is exactly one provider-specific rule in the storage layer, and it is
called out here so that it stays the only one: recognising a Garage bucket
quota by the wording of its `AccessDenied` message. S3 has no quota concept, so
there is nothing portable to match on, and AWS and R2 have no per-bucket quota
to report at all. It is acceptable only because it can only ever reclassify a
refusal from "permission denied" to "bucket full", never the reverse, so a
provider it does not understand gets the safe answer. See ADR 0002 section 5.
- **`depends_on: garage` from the backend.** Since Compose 2.20, depending on a
  profiled service re-enables its profile, so that edge would start the bundled
  object store for an operator running against AWS. The backend does a
  `HeadBucket` on both buckets at boot and fails fast instead.
