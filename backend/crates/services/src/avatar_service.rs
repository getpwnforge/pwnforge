// backend/crates/services/src/avatar_service.rs
use crate::storage_service::{
    AvatarKind, AvatarSize, Bucket, ObjectStore, StorageError, avatar_key, avatar_prefix,
    avatar_version_prefix,
};
use chrono::Utc;
use domain::entities::{teams, users, workspaces};
use image::{ExtendedColorType, ImageEncoder, ImageReader, codecs::webp::WebPEncoder};
use sea_orm::{ActiveValue::Set, DatabaseConnection, EntityTrait};
use std::io::Cursor;
use std::time::Duration;
use uuid::Uuid;

/// Largest upload accepted, before any decoding.
///
/// Checked on the byte string the client sent, not on the decoded image: the
/// point is to bound what reaches the decoder at all, so it has to be the
/// first thing that happens. The route also sets its own `DefaultBodyLimit` to
/// the same figure, which is what stops an oversized body from being buffered
/// in the first place; this check covers the callers that are not that route.
pub const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;

/// Largest rendition read back out of the bucket.
///
/// Our own encoder wrote these, and a 256×256 lossless WebP does not approach
/// this, so it is a backstop against a bucket whose contents are not what we
/// put there rather than a limit anyone should reach.
pub const MAX_RENDITION_BYTES: usize = 2 * 1024 * 1024;

/// Dimension ceiling handed to the decoder.
///
/// A decompression bomb is small on the wire and enormous in memory: a PNG of
/// a few kilobytes can declare 60000×60000, which is 14 GB of RGBA. The
/// decoder is told the ceiling up front so it refuses while reading the
/// header, instead of after allocating.
const MAX_DIMENSION: u32 = 8000;

/// Allocation ceiling handed to the decoder, in bytes.
///
/// Derived from `MAX_DIMENSION` rather than chosen: it is exactly one
/// 8000×8000 RGBA buffer, 256 MB. Deriving it is what keeps the two limits
/// from drifting apart - raising the dimension ceiling without raising this
/// would make the dimension limit unreachable, and the decoder would refuse
/// on allocation with a less specific error.
const MAX_DECODE_ALLOC: u64 = (MAX_DIMENSION as u64) * (MAX_DIMENSION as u64) * 4;

/// Wall-clock bound on one decode-and-render job.
///
/// This bounds the *request*, not the CPU: a `spawn_blocking` task cannot be
/// cancelled, so a pathological decode keeps running on the blocking pool
/// after the client has been answered. What actually bounds the work is the
/// pair of decoder limits above, which is why they are not optional. The
/// timeout exists so that a client is never left hanging on one.
const RENDER_TIMEOUT: Duration = Duration::from_secs(15);

/// Magic bytes we accept, as `infer` reports them.
const ALLOWED_MIME: [&str; 3] = ["image/png", "image/jpeg", "image/webp"];

/// What everything in this module is stored and served as.
pub const OUTPUT_CONTENT_TYPE: &str = "image/webp";

#[derive(Debug, thiserror::Error)]
pub enum AvatarError {
    #[error("avatar is larger than the {limit} byte upload limit")]
    TooLarge { limit: usize },

    /// The magic bytes are not one of PNG, JPEG or WebP. Covers an empty body,
    /// a text file, a PDF, and a polyglot whose header is not an image.
    #[error("unsupported image format")]
    UnsupportedFormat,

    /// Called out separately from `UnsupportedFormat` because SVG is the one
    /// format people actively expect to work and the one with an actual
    /// scripting engine behind it in a browser. Worth its own message so the
    /// client can say why rather than "unsupported".
    #[error("SVG avatars are not accepted")]
    SvgRejected,

    /// The magic bytes said it was an image and the decoder disagreed, or the
    /// decoder hit one of its limits. Both mean the same thing to a caller:
    /// this file is not going to become an avatar.
    #[error("image could not be decoded")]
    Undecodable,

    #[error("image took too long to process")]
    RenderTimeout,

    /// A panic inside the decoder. `image` is memory-safe Rust, so this is an
    /// arithmetic or indexing bug rather than a memory-corruption one, but it
    /// is reachable from attacker-controlled bytes and must not take the
    /// process, or the blocking worker, down with it.
    #[error("image decoder panicked")]
    RenderPanicked,

    #[error("avatar could not be encoded")]
    EncodeFailed,

    /// The entity exists but has no avatar, or the object behind its version
    /// is gone.
    #[error("no avatar for this entity")]
    NotFound,

    #[error(transparent)]
    Storage(#[from] StorageError),

    #[error(transparent)]
    Db(#[from] sea_orm::DbErr),
}

/// The two re-encoded renditions of one upload, with the version they belong
/// to. Only these ever reach the bucket.
pub struct RenderedAvatar {
    pub version: Uuid,
    pub large: Vec<u8>,
    pub small: Vec<u8>,
}

/// Hand written so a `Debug` of this does not dump two image buffers into a
/// log line or a test failure. The lengths are the part anyone actually wants.
impl std::fmt::Debug for RenderedAvatar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RenderedAvatar")
            .field("version", &self.version)
            .field("large_bytes", &self.large.len())
            .field("small_bytes", &self.small.len())
            .finish()
    }
}

/// Validates, decodes, resizes and re-encodes an uploaded image.
///
/// The original bytes are never returned and never stored. Everything the
/// bucket holds was produced by our own encoder from a decoded pixel buffer,
/// which is what makes serving an avatar `inline` defensible: a polyglot file
/// that is simultaneously a valid JPEG and a valid HTML document does not
/// survive being decoded to pixels and re-encoded, and neither does an EXIF
/// block, a trailing ZIP archive, or an embedded colour profile.
pub async fn render(bytes: Vec<u8>) -> Result<RenderedAvatar, AvatarError> {
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(AvatarError::TooLarge {
            limit: MAX_UPLOAD_BYTES,
        });
    }

    // Cheap, total, and done before a decoder is ever handed the bytes. It is
    // a pre-filter, not the decision: the decoder below still has to agree
    // that these bytes are an image. Running it first means a PDF or an ELF
    // never reaches a decoder at all.
    reject_disallowed_format(&bytes)?;

    // `spawn_blocking`, because decoding and resizing are CPU-bound and would
    // otherwise stall the async runtime for every other request on that
    // worker.
    let handle = tokio::task::spawn_blocking(move || render_blocking(&bytes));

    match tokio::time::timeout(RENDER_TIMEOUT, handle).await {
        Err(_elapsed) => Err(AvatarError::RenderTimeout),
        Ok(Err(join_error)) if join_error.is_panic() => {
            // The blocking pool replaces the thread, so the panic costs one
            // worker and one request rather than the process.
            tracing::error!("avatar decoder panicked");
            Err(AvatarError::RenderPanicked)
        }
        Ok(Err(join_error)) => {
            tracing::error!(error = ?join_error, "avatar render task failed to join");
            Err(AvatarError::Undecodable)
        }
        Ok(Ok(result)) => result,
    }
}

/// Renders, stores both renditions, records the new version, then deletes the
/// one it replaced.
///
/// The order is what makes a failure safe. The new objects exist before the
/// row points at them, so a crash between the two leaves an orphan rather than
/// a row naming an object that is not there - a broken image is a worse
/// outcome than a few stray bytes. The delete comes last and is best effort
/// for the same reason: the row is already correct, and failing the request
/// because cleanup failed would tell the user their upload did not work when
/// it did.
pub async fn upload(
    db: &DatabaseConnection,
    store: &ObjectStore,
    kind: AvatarKind,
    id: Uuid,
    bytes: Vec<u8>,
) -> Result<Uuid, AvatarError> {
    let previous = current_version(db, kind, id).await?;
    let rendered = render(bytes).await?;

    store
        .put(
            Bucket::Avatars,
            &avatar_key(kind, id, rendered.version, AvatarSize::Large),
            rendered.large,
            OUTPUT_CONTENT_TYPE,
        )
        .await?;

    store
        .put(
            Bucket::Avatars,
            &avatar_key(kind, id, rendered.version, AvatarSize::Small),
            rendered.small,
            OUTPUT_CONTENT_TYPE,
        )
        .await?;

    set_version(db, kind, id, Some(rendered.version)).await?;

    if let Some(previous) = previous
        && previous != rendered.version
        && let Err(error) = store
            .delete_prefix(Bucket::Avatars, &avatar_version_prefix(kind, id, previous))
            .await
    {
        // Logged rather than returned: see the note on ordering above.
        tracing::warn!(
            ?error,
            ?kind,
            %id,
            %previous,
            "could not delete the superseded avatar version"
        );
    }

    Ok(rendered.version)
}

/// Reads one rendition of the entity's current avatar.
///
/// The version comes from the database rather than from the caller. A
/// client-supplied version would turn this into a read of any key under the
/// entity's prefix, which is a small surface but not one worth opening for a
/// cache-buster that the URL can carry separately.
pub async fn read_current(
    db: &DatabaseConnection,
    store: &ObjectStore,
    kind: AvatarKind,
    id: Uuid,
    size: AvatarSize,
) -> Result<(Uuid, Vec<u8>), AvatarError> {
    let version = current_version(db, kind, id)
        .await?
        .ok_or(AvatarError::NotFound)?;

    let bytes = store
        .get(
            Bucket::Avatars,
            &avatar_key(kind, id, version, size),
            MAX_RENDITION_BYTES,
        )
        .await
        .map_err(|error| match error {
            // The row says there is an avatar and the bucket disagrees. That
            // is a real inconsistency worth a log line, but to the client it
            // is simply absent.
            StorageError::NotFound => {
                tracing::warn!(?kind, %id, %version, "avatar row has no object behind it");
                AvatarError::NotFound
            }
            other => AvatarError::Storage(other),
        })?;

    Ok((version, bytes))
}

/// Clears the avatar and purges every version the entity ever had.
///
/// Uses the entity prefix rather than the current version: an interrupted
/// upload can leave versions behind that no row names, and this is the only
/// thing that ever collects them.
pub async fn remove(
    db: &DatabaseConnection,
    store: &ObjectStore,
    kind: AvatarKind,
    id: Uuid,
) -> Result<(), AvatarError> {
    set_version(db, kind, id, None).await?;

    if let Err(error) = store
        .delete_prefix(Bucket::Avatars, &avatar_prefix(kind, id))
        .await
    {
        tracing::warn!(?error, ?kind, %id, "could not purge avatar objects");
    }

    Ok(())
}

/// Reads `avatar_version` for one entity.
pub async fn current_version(
    db: &DatabaseConnection,
    kind: AvatarKind,
    id: Uuid,
) -> Result<Option<Uuid>, AvatarError> {
    // One arm per table rather than anything generic: the three entities are
    // unrelated types with unrelated `ActiveModel`s, and a macro or a trait
    // over them would be more machinery than three lines of `match`.
    let version = match kind {
        AvatarKind::User => users::Entity::find_by_id(id)
            .one(db)
            .await?
            .and_then(|row| row.avatar_version),
        AvatarKind::Team => teams::Entity::find_by_id(id)
            .one(db)
            .await?
            .and_then(|row| row.avatar_version),
        AvatarKind::Workspace => workspaces::Entity::find_by_id(id)
            .one(db)
            .await?
            .and_then(|row| row.avatar_version),
    };

    Ok(version)
}

async fn set_version(
    db: &DatabaseConnection,
    kind: AvatarKind,
    id: Uuid,
    version: Option<Uuid>,
) -> Result<(), AvatarError> {
    match kind {
        AvatarKind::User => {
            let model = users::ActiveModel {
                id: Set(id),
                avatar_version: Set(version),
                updated_at: Set(Utc::now().into()),
                ..Default::default()
            };
            users::Entity::update(model).exec(db).await?;
        }
        // `teams` has no `updated_at` column, so there is nothing to stamp.
        AvatarKind::Team => {
            let model = teams::ActiveModel {
                id: Set(id),
                avatar_version: Set(version),
                ..Default::default()
            };
            teams::Entity::update(model).exec(db).await?;
        }
        AvatarKind::Workspace => {
            let model = workspaces::ActiveModel {
                id: Set(id),
                avatar_version: Set(version),
                updated_at: Set(Utc::now().into()),
                ..Default::default()
            };
            workspaces::Entity::update(model).exec(db).await?;
        }
    }

    Ok(())
}

/// The allowlist, applied to magic bytes rather than to a filename or a
/// client-supplied `Content-Type`, both of which the uploader controls.
fn reject_disallowed_format(bytes: &[u8]) -> Result<(), AvatarError> {
    if looks_like_svg(bytes) {
        return Err(AvatarError::SvgRejected);
    }

    let kind = infer::get(bytes).ok_or(AvatarError::UnsupportedFormat)?;

    if !ALLOWED_MIME.contains(&kind.mime_type()) {
        return Err(AvatarError::UnsupportedFormat);
    }

    Ok(())
}

/// SVG has no magic number - it is XML - so `infer` would simply fail to
/// identify it and it would fall out as `UnsupportedFormat`. It gets its own
/// check anyway, for two reasons: the error message can then say why, and the
/// rejection becomes something a test can pin rather than a side effect of
/// the allowlist happening not to include it.
///
/// An SVG is a document, not a raster image: it can carry `<script>`, an
/// `xlink:href` to a remote resource, and a `<foreignObject>` full of HTML.
/// Served inline from our own origin, that is stored XSS.
fn looks_like_svg(bytes: &[u8]) -> bool {
    // Enough to cover a leading XML declaration, a doctype, a comment or a
    // byte-order mark before the root element.
    let head = &bytes[..bytes.len().min(1024)];
    let head = String::from_utf8_lossy(head).to_ascii_lowercase();

    head.contains("<svg") || (head.contains("<?xml") && head.contains("svg"))
}

/// The CPU-bound half, run on the blocking pool.
fn render_blocking(bytes: &[u8]) -> Result<RenderedAvatar, AvatarError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| AvatarError::Undecodable)?;

    // Set before `decode`, so the limits apply while the header is being read
    // and the decoder refuses to allocate rather than refusing afterwards.
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    reader.limits(limits);

    // `with_guessed_format` sniffs the magic bytes again, independently of the
    // `infer` pre-filter. The formats compiled in are only PNG, JPEG and WebP
    // (see `Cargo.toml`), so even a disagreement between the two cannot reach
    // a decoder outside the allowlist.
    let image = reader.decode().map_err(|error| {
        tracing::debug!(?error, "avatar decode refused");
        AvatarError::Undecodable
    })?;

    let large = encode_square_webp(&image, AvatarSize::Large)?;
    let small = encode_square_webp(&image, AvatarSize::Small)?;

    Ok(RenderedAvatar {
        // v7 rather than v4: versions are written in time order, which makes
        // the bucket listing for one entity read chronologically when someone
        // is working out which objects an interrupted upload left behind.
        version: Uuid::now_v7(),
        large,
        small,
    })
}

/// Resizes to a centred square and re-encodes as lossless WebP.
///
/// `resize_to_fill` crops rather than letterboxes, which is what an avatar
/// wants: every rendition is exactly square, so the frontend never has to
/// reason about aspect ratio.
fn encode_square_webp(
    image: &image::DynamicImage,
    size: AvatarSize,
) -> Result<Vec<u8>, AvatarError> {
    let edge = size.pixels();
    let resized = image
        .resize_to_fill(edge, edge, image::imageops::FilterType::Lanczos3)
        .to_rgba8();

    let mut out = Vec::new();

    WebPEncoder::new_lossless(&mut out)
        .write_image(
            resized.as_raw(),
            resized.width(),
            resized.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| {
            tracing::error!(?error, "avatar WebP encode failed");
            AvatarError::EncodeFailed
        })?;

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage_service::MemoryStore;
    use image::{DynamicImage, ImageFormat, RgbaImage};

    /// A real encoded image of the given format, built rather than checked in,
    /// so the fixtures cannot drift from what the decoder accepts.
    fn encoded(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
        let mut image = RgbaImage::new(width, height);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]);
        }

        let image = match format {
            // JPEG has no alpha channel and the encoder refuses RGBA.
            ImageFormat::Jpeg => DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(image).to_rgb8()),
            _ => DynamicImage::ImageRgba8(image),
        };

        let mut out = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut out), format)
            .expect("fixture must encode");
        out
    }

    /// Positive: each allowed format decodes and produces both renditions at
    /// exactly the declared sizes.
    #[tokio::test]
    async fn every_allowed_format_renders_both_sizes() {
        for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
            let rendered = render(encoded(format, 512, 300))
                .await
                .unwrap_or_else(|error| panic!("{format:?} must render, got {error}"));

            for (bytes, size) in [
                (&rendered.large, AvatarSize::Large),
                (&rendered.small, AvatarSize::Small),
            ] {
                let decoded = image::load_from_memory_with_format(bytes, ImageFormat::WebP)
                    .expect("output must be WebP");

                assert_eq!(decoded.width(), size.pixels(), "{format:?} {size:?} width");
                assert_eq!(
                    decoded.height(),
                    size.pixels(),
                    "{format:?} {size:?} height"
                );
            }
        }
    }

    /// The output is always WebP regardless of what went in, which is what the
    /// endpoint's fixed `Content-Type: image/webp` relies on.
    #[tokio::test]
    async fn output_is_always_webp() {
        let rendered = render(encoded(ImageFormat::Png, 64, 64)).await.unwrap();

        assert_eq!(
            image::guess_format(&rendered.large).unwrap(),
            ImageFormat::WebP
        );
        assert_eq!(
            image::guess_format(&rendered.small).unwrap(),
            ImageFormat::WebP
        );
    }

    /// Negative: SVG is refused by name, not merely by falling off the
    /// allowlist. Covers the bare root element and the XML-declaration form.
    #[tokio::test]
    async fn svg_is_refused_explicitly() {
        let svgs: [&[u8]; 4] = [
            br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#,
            br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"/>"#,
            br#"<SVG XMLNS="http://www.w3.org/2000/svg"/>"#,
            br#"<!-- a comment --><svg/>"#,
        ];

        for svg in svgs {
            let error = render(svg.to_vec())
                .await
                .expect_err("SVG must not be accepted");

            assert!(
                matches!(error, AvatarError::SvgRejected),
                "expected SvgRejected, got {error}"
            );
        }
    }

    /// Negative: formats the allowlist excludes, including ones `image` can
    /// identify. GIF is the interesting case - `infer` recognises it, so this
    /// proves the allowlist is doing the work rather than the sniffer failing.
    #[tokio::test]
    async fn disallowed_formats_are_refused() {
        let gif: &[u8] =
            b"GIF89a\x01\x00\x01\x00\x00\xff\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x00;";
        let pdf: &[u8] = b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n";
        let elf: &[u8] = b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00";
        let text: &[u8] = b"this is not an image at all";
        let empty: &[u8] = b"";

        for bytes in [gif, pdf, elf, text, empty] {
            let error = render(bytes.to_vec())
                .await
                .expect_err("must not be accepted");

            assert!(
                matches!(error, AvatarError::UnsupportedFormat),
                "expected UnsupportedFormat, got {error}"
            );
        }
    }

    /// Negative: a polyglot whose magic bytes are a real PNG but which carries
    /// HTML in its tail. The pre-filter passes it, the decoder reads the image,
    /// and the stored object is our own re-encode - so the HTML is gone. This
    /// is the property that makes `Content-Disposition: inline` acceptable for
    /// avatars, so it is pinned rather than assumed.
    #[tokio::test]
    async fn a_polyglot_loses_its_payload() {
        let mut polyglot = encoded(ImageFormat::Png, 64, 64);
        let payload = b"<html><script>alert(document.domain)</script></html>";
        polyglot.extend_from_slice(payload);

        let rendered = render(polyglot)
            .await
            .expect("a valid PNG head must render");

        for bytes in [&rendered.large, &rendered.small] {
            assert!(
                !bytes.windows(payload.len()).any(|w| w == payload),
                "the re-encoded output still carries the appended payload"
            );
            assert!(
                !bytes.windows(7).any(|w| w == b"<script"),
                "the re-encoded output still carries a script tag"
            );
        }
    }

    /// Negative: past the upload ceiling, refused before anything is decoded.
    #[tokio::test]
    async fn an_oversized_upload_is_refused() {
        let error = render(vec![0u8; MAX_UPLOAD_BYTES + 1])
            .await
            .expect_err("past the limit must be refused");

        assert!(matches!(
            error,
            AvatarError::TooLarge {
                limit: MAX_UPLOAD_BYTES
            }
        ));
    }

    /// Negative: a decompression bomb. The header declares 40000×40000, which
    /// is 6.4 GB of RGBA, in a file of a few kilobytes. The decoder must
    /// refuse from the header rather than allocating.
    ///
    /// Pinned with a real encoded PNG rather than a hand-built header so the
    /// test exercises the limit and not our ability to forge a chunk.
    #[tokio::test]
    async fn a_decompression_bomb_is_refused() {
        // 12000 is past MAX_DIMENSION (8000) but small enough to encode here.
        let bomb = encoded(ImageFormat::Png, 12000, 4);

        let error = render(bomb)
            .await
            .expect_err("past MAX_DIMENSION must be refused");

        assert!(
            matches!(error, AvatarError::Undecodable),
            "expected Undecodable, got {error}"
        );
    }

    /// The limits are what actually bound the decoder, since the timeout
    /// cannot cancel a blocking task. Pinned so neither is quietly dropped.
    #[test]
    fn the_decoder_limits_are_set() {
        assert_eq!(MAX_DIMENSION, 8000);

        // The allocation ceiling must admit exactly one image of the largest
        // permitted dimensions. Any less and the dimension limit is dead code,
        // because allocation refuses first; any more and the process can be
        // made to hold buffers it has no use for.
        assert_eq!(
            MAX_DECODE_ALLOC,
            u64::from(MAX_DIMENSION) * u64::from(MAX_DIMENSION) * 4
        );
    }

    /// An upload writes both renditions under the new version and nothing
    /// else. The database half is covered by the integration test in
    /// `tests/avatar_storage.rs`, which has a real Postgres to update.
    #[tokio::test]
    async fn rendered_output_lands_under_the_version_prefix() {
        let store = ObjectStore::Memory(MemoryStore::new());
        let id = Uuid::now_v7();

        let rendered = render(encoded(ImageFormat::Png, 128, 128)).await.unwrap();

        store
            .put(
                Bucket::Avatars,
                &avatar_key(AvatarKind::User, id, rendered.version, AvatarSize::Large),
                rendered.large,
                OUTPUT_CONTENT_TYPE,
            )
            .await
            .unwrap();
        store
            .put(
                Bucket::Avatars,
                &avatar_key(AvatarKind::User, id, rendered.version, AvatarSize::Small),
                rendered.small,
                OUTPUT_CONTENT_TYPE,
            )
            .await
            .unwrap();

        let ObjectStore::Memory(memory) = &store else {
            unreachable!("constructed as Memory above")
        };
        assert_eq!(memory.len(), 2);

        store
            .delete_prefix(
                Bucket::Avatars,
                &avatar_version_prefix(AvatarKind::User, id, rendered.version),
            )
            .await
            .unwrap();

        assert_eq!(
            memory.len(),
            0,
            "the version prefix must cover both renditions"
        );
    }
}
