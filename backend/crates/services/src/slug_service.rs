// backend/crates/services/src/slug_service.rs

use std::future::Future;

use sea_orm::DbErr;

/// Normalizes a string into a slug: lowercase, accents stripped, anything
/// that is not alphanumeric replaced by a hyphen, leading/trailing/repeated
/// hyphens collapsed.
pub fn slugify(input: &str) -> String {
    deunicode::deunicode(input)
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Slugs that would shadow a static segment of a collection route.
///
/// `/teams/{slug}` sits alongside `/teams/join` and
/// `/teams/invitations/...`, so a team slugged `join` competes with a real
/// endpoint for the same path. The router resolves static segments first, so
/// the team is simply unreachable, and a future route added under the same
/// prefix silently strands another team.
///
/// Reserved across every sluggable resource rather than per table: workspaces
/// and organizations will hang off the same shape of URL, and a slug that is
/// safe today should not become a trap when the next collection lands.
pub const RESERVED_SLUGS: &[&str] = &[
    "join",
    "invitations",
    "members",
    "new",
    "me",
    "settings",
    "admin",
    "api",
    "search",
];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SlugError {
    /// The name held nothing a slug can be built from: punctuation, emoji, or
    /// a script `deunicode` cannot transliterate. Refused rather than stored
    /// empty, which would produce `/teams/` and a permanently unreachable row.
    #[error("name must contain at least one letter or digit")]
    Empty,

    #[error("name resolves to a reserved slug")]
    Reserved,

    #[error(transparent)]
    Db(#[from] DbErr),
}

/// Validates the slug a name would produce, without touching the database.
///
/// Called on create *and* on rename: a team renamed to `join` is exactly as
/// unreachable as one created that way, and enforcing this only on the create
/// path would leave the rename route as the way around it.
pub fn validate_slug_source(name: &str) -> Result<String, SlugError> {
    let slug = slugify(name);

    if slug.is_empty() {
        return Err(SlugError::Empty);
    }

    if RESERVED_SLUGS.contains(&slug.as_str()) {
        return Err(SlugError::Reserved);
    }

    Ok(slug)
}

/// Generates a unique slug, retrying with a `-2`, `-3`, ... suffix on
/// collision. `exists` is supplied by the caller so this stays agnostic of
/// the target table (`teams.slug`, `workspaces.slug`, `organizations.slug`,
/// ...), `slug_service` does not depend on any domain entity.
pub async fn generate_unique_slug<F, Fut>(input: &str, exists: F) -> Result<String, SlugError>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<bool, DbErr>>,
{
    let base = validate_slug_source(input)?;

    if !exists(base.clone()).await? {
        return Ok(base);
    }

    for n in 2..1000 {
        let candidate = format!("{base}-{n}");
        if !exists(candidate.clone()).await? {
            return Ok(candidate);
        }
    }

    Err(SlugError::Db(DbErr::Custom(
        "slug generation exhausted retry limit".into(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Positive: ordinary names slugify as expected, accents folded.
    #[test]
    fn slugify_normalizes_a_readable_name() {
        assert_eq!(slugify("Les Pâtissiers"), "les-patissiers");
        assert_eq!(slugify("  Team   Alpha  "), "team-alpha");
        assert_eq!(slugify("CTF/2026"), "ctf-2026");
    }

    /// Negative: a name made only of punctuation collapses to nothing. Stored
    /// as-is it would produce `/teams/`, which matches no route, leaving the
    /// team impossible to open for as long as it exists.
    #[test]
    fn a_name_without_alphanumerics_is_refused() {
        for name in ["...", "---", "!!!", "   ", "@#$%"] {
            assert_eq!(
                validate_slug_source(name),
                Err(SlugError::Empty),
                "{name:?} must not yield a usable slug"
            );
        }
    }

    /// Negative: names that collide with a static route segment are refused.
    #[test]
    fn a_name_colliding_with_a_route_segment_is_refused() {
        for name in ["join", "Join", "Invitations", "members"] {
            assert_eq!(
                validate_slug_source(name),
                Err(SlugError::Reserved),
                "{name:?} must be refused as a reserved slug"
            );
        }
    }

    /// A reserved word is only reserved on its own: it may still appear inside
    /// a longer name, which produces a slug that collides with nothing.
    #[test]
    fn a_reserved_word_inside_a_longer_name_is_allowed() {
        assert_eq!(
            validate_slug_source("Join The Party"),
            Ok("join-the-party".to_owned())
        );
    }

    #[tokio::test]
    async fn generate_unique_slug_suffixes_on_collision() {
        let slug = generate_unique_slug("Team Alpha", |candidate| async move {
            Ok(candidate == "team-alpha")
        })
        .await
        .expect("a suffixed slug must be found");

        assert_eq!(slug, "team-alpha-2");
    }

    #[tokio::test]
    async fn generate_unique_slug_refuses_an_empty_source() {
        let result = generate_unique_slug("...", |_| async { Ok(false) }).await;

        assert_eq!(result, Err(SlugError::Empty));
    }
}
