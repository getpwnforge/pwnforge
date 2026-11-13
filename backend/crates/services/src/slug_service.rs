// backend/crates/services/src/slug_service.rs

use std::future::Future;

use rand::RngExt;
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

    /// Every candidate drawn was already taken. Reachable only if the random
    /// suffix loses ten draws in a row against rows sharing the same base
    /// slug, so in practice it means something else is wrong: a stuck RNG, or
    /// an `exists` closure that answers `true` unconditionally. Surfaced as
    /// its own variant rather than folded into a generic database failure, so
    /// that when it does happen it is legible in the response and in the logs
    /// instead of looking like a transient query error.
    #[error("no free slug after the maximum number of attempts")]
    Exhausted,

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

/// The characters a suffix is drawn from: lowercase alphanumerics, minus the
/// five that are read or copied wrong off a screen. `0`/`o` and `1`/`l`/`i`
/// are indistinguishable in many fonts, and a slug is something people read
/// out of a URL bar and type back in, so the pairs are worth the smaller
/// alphabet. 31 characters, which is enough: see the doc on the constants
/// below.
const SUFFIX_ALPHABET: &[u8] = b"23456789abcdefghjkmnpqrstuvwxyz";

/// 31^4 is roughly 920 000 suffixes per base slug, so five draws collide only
/// if a base is already shared by a large number of rows.
const SUFFIX_LEN: usize = 4;

/// The fallback length, tried only once the short ones are exhausted. 31^6 is
/// roughly 880 million: reaching it means the base is saturated well past
/// anything a real instance produces, and the caller deserves an error rather
/// than a loop that keeps drawing.
const SUFFIX_LEN_FALLBACK: usize = 6;

/// Draws per length. Bounded on purpose: each draw is a database round trip,
/// and an `exists` closure stuck on `true` must terminate rather than spin.
const ATTEMPTS_PER_LENGTH: usize = 5;

/// Generates a unique slug of the form `<base>-<suffix>`, e.g. `hexagon-x7k9`.
///
/// The base is **never** returned on its own, even when it is free. Returning
/// it bare exactly when nothing holds it turns creation into an existence
/// oracle: ask for "Hexagon", read the slug you were given back, and the
/// presence of a suffix tells you whether a team or workspace named Hexagon
/// already exists somewhere on the instance. `workspaces.slug` is unique
/// instance-wide, so that answer spans every private workspace of every team,
/// which is precisely what a non-member must not be able to probe. Suffixing
/// unconditionally removes the signal: every answer looks the same.
///
/// The suffix is drawn fresh at each attempt, from the same CSPRNG the token
/// service uses. Deliberately not a counter (`-2`, `-3`, ...): a counter
/// leaks the same thing in finer detail, since its value is the number of
/// namesakes. Deliberately not derived from the row's UUID either: the id is
/// generated before the insert, so anyone who can observe or predict it could
/// derive the URL of a workspace they cannot read.
///
/// `exists` is supplied by the caller, so this stays agnostic of the target
/// table (`teams.slug`, `workspaces.slug`, ...) and `slug_service` depends on
/// no domain entity. It is called once per attempt, with the full candidate.
///
/// This is a best effort, not a lock: two concurrent calls can draw two
/// candidates that both look free. The unique index on the column is what
/// actually settles it, and the loser gets a constraint violation to retry.
pub async fn generate_unique_slug<F, Fut>(input: &str, exists: F) -> Result<String, SlugError>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<bool, DbErr>>,
{
    let base = validate_slug_source(input)?;

    for len in [SUFFIX_LEN, SUFFIX_LEN_FALLBACK] {
        for _ in 0..ATTEMPTS_PER_LENGTH {
            let candidate = format!("{base}-{}", random_suffix(len));

            if !exists(candidate.clone()).await? {
                return Ok(candidate);
            }
        }
    }

    Err(SlugError::Exhausted)
}

/// A fresh random suffix of `len` characters, independent of every previous
/// draw and of anything about the row being created.
fn random_suffix(len: usize) -> String {
    let mut rng = rand::rng();

    (0..len)
        .map(|_| SUFFIX_ALPHABET[rng.random_range(0..SUFFIX_ALPHABET.len())] as char)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        sync::{Arc, Mutex},
    };

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
    async fn generate_unique_slug_refuses_an_empty_source() {
        let result = generate_unique_slug("...", |_| async { Ok(false) }).await;

        assert_eq!(result, Err(SlugError::Empty));
    }

    /// `^<base>-[23456789a-hjkmnp-z]{n}$`, spelled out rather than compiled:
    /// the `regex` crate is not a dependency of this workspace, and one
    /// assertion does not justify adding one.
    fn matches_slug_format(slug: &str, base: &str, suffix_len: usize) -> bool {
        let Some(suffix) = slug.strip_prefix(&format!("{base}-")) else {
            return false;
        };

        suffix.chars().count() == suffix_len
            && suffix
                .chars()
                .all(|c| c.is_ascii() && SUFFIX_ALPHABET.contains(&(c as u8)))
    }

    /// Every candidate the generator tried, in order.
    type CandidateLog = Arc<Mutex<Vec<String>>>;

    /// `exists` answers without touching a database, so the futures it hands
    /// back are already resolved.
    type ExistsAnswer = std::future::Ready<Result<bool, DbErr>>;

    /// Records every candidate the generator tried, so a test can assert on
    /// what was asked rather than only on what came back.
    fn recording_exists(taken: usize) -> (CandidateLog, impl Fn(String) -> ExistsAnswer) {
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let log = Arc::clone(&seen);

        let exists = move |candidate: String| {
            let mut log = log.lock().expect("the log mutex is never poisoned");
            log.push(candidate);

            // The first `taken` candidates are answered as already used.
            std::future::ready(Ok(log.len() <= taken))
        };

        (seen, exists)
    }

    /// The point of the whole design: a base nobody holds is *still* suffixed.
    /// Returning it bare here would make the shape of the answer depend on
    /// whether the name is already taken somewhere, which is the existence
    /// oracle the suffix exists to close.
    #[tokio::test]
    async fn a_free_base_is_suffixed_anyway() {
        let (seen, exists) = recording_exists(0);

        let slug = generate_unique_slug("Team Alpha", exists)
            .await
            .expect("a free base must still produce a slug");

        assert_ne!(slug, "team-alpha", "the bare base must never be returned");
        assert!(
            matches_slug_format(&slug, "team-alpha", SUFFIX_LEN),
            "{slug:?} must be the base plus a {SUFFIX_LEN}-character suffix"
        );

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "a free candidate is accepted on sight");
        assert_ne!(
            seen[0], "team-alpha",
            "the bare base must not even be looked up: querying it and \
             querying a suffixed candidate are distinguishable from outside"
        );
    }

    /// A taken base goes down the same path and produces the same shape. The
    /// two cases differ only in how many candidates get drawn, never in what
    /// the caller receives.
    #[tokio::test]
    async fn a_taken_base_takes_the_same_path() {
        let (seen, exists) = recording_exists(0);
        let free = generate_unique_slug("Team Alpha", exists).await.unwrap();
        let free_attempts = seen.lock().unwrap().len();

        // Same call, except the first candidate drawn is already held.
        let (seen, exists) = recording_exists(1);
        let taken = generate_unique_slug("Team Alpha", exists).await.unwrap();

        assert!(matches_slug_format(&free, "team-alpha", SUFFIX_LEN));
        assert!(
            matches_slug_format(&taken, "team-alpha", SUFFIX_LEN),
            "{taken:?} must have the same shape as the free-base result"
        );
        assert_eq!(free_attempts, 1);
        assert_eq!(seen.lock().unwrap().len(), 2, "one collision, one redraw");
    }

    /// A suffix that collides is redrawn, and the redraw is a new random
    /// suffix rather than a mutation of the previous one.
    #[tokio::test]
    async fn a_colliding_suffix_is_redrawn() {
        let (seen, exists) = recording_exists(3);

        let slug = generate_unique_slug("Team Alpha", exists)
            .await
            .expect("the fourth draw is free");

        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 4, "three collisions, then a free candidate");
        assert_eq!(*seen.last().unwrap(), slug);
        assert!(
            seen.iter()
                .all(|c| matches_slug_format(c, "team-alpha", SUFFIX_LEN)),
            "every candidate keeps the same shape: {seen:?}"
        );

        let distinct: HashSet<&String> = seen.iter().collect();
        assert_eq!(
            distinct.len(),
            seen.len(),
            "each attempt draws a fresh suffix, not a counter: {seen:?}"
        );
    }

    /// Negative: when nothing is ever free the loop stops, at a known count,
    /// with an error of its own. The failure mode this rules out is a
    /// generator that spins forever against an `exists` stuck on `true`.
    #[tokio::test]
    async fn an_always_taken_slug_gives_up_loudly() {
        let (seen, exists) = recording_exists(usize::MAX);

        let result = generate_unique_slug("Team Alpha", exists).await;

        assert_eq!(result, Err(SlugError::Exhausted));

        let seen = seen.lock().unwrap();
        // Ten, written out rather than derived from the constants: a test that
        // recomputes the budget from the same constants it is meant to pin
        // follows them wherever they are edited to.
        assert_eq!(seen.len(), 10, "five draws per length, two lengths");
        assert!(
            seen[..ATTEMPTS_PER_LENGTH]
                .iter()
                .all(|c| matches_slug_format(c, "team-alpha", SUFFIX_LEN)),
            "the short length is tried first: {seen:?}"
        );
        assert!(
            seen[ATTEMPTS_PER_LENGTH..]
                .iter()
                .all(|c| matches_slug_format(c, "team-alpha", SUFFIX_LEN_FALLBACK)),
            "then the fallback length, once and not before: {seen:?}"
        );
    }

    /// The ambiguous glyphs never appear. Sampled rather than reasoned about,
    /// because the alphabet is a constant someone will edit one day, and a
    /// stray `o` in it is invisible on review.
    #[test]
    fn a_generated_suffix_holds_no_ambiguous_character() {
        let mut drawn = HashSet::new();

        for _ in 0..1_000 {
            let suffix = random_suffix(SUFFIX_LEN);

            assert_eq!(suffix.chars().count(), SUFFIX_LEN);
            for c in suffix.chars() {
                assert!(
                    !matches!(c, '0' | 'o' | '1' | 'l' | 'i'),
                    "{suffix:?} contains the ambiguous character {c:?}"
                );
                assert!(
                    c.is_ascii_lowercase() || c.is_ascii_digit(),
                    "{suffix:?} contains {c:?}, outside the alphabet"
                );
                drawn.insert(c);
            }
        }

        assert!(
            drawn.len() > 20,
            "1000 draws should cover most of a 31-character alphabet, saw {}: \
             a much smaller spread means the draw is not uniform",
            drawn.len()
        );
    }

    /// The numbers themselves, as literals. Every other test here reads the
    /// constants, so it would follow them to whatever they were edited to;
    /// this one is what makes changing the budget a deliberate act.
    #[test]
    fn the_generation_budget_is_the_documented_one() {
        assert_eq!(SUFFIX_LEN, 4);
        assert_eq!(SUFFIX_LEN_FALLBACK, 6);
        assert_eq!(ATTEMPTS_PER_LENGTH, 5);
        assert_eq!(
            SUFFIX_ALPHABET.len(),
            31,
            "36 lowercase alphanumerics, minus 0, o, 1, l and i"
        );
    }

    /// The whole slug, not just the suffix, against the format callers and
    /// the router depend on.
    #[tokio::test]
    async fn the_generated_slug_matches_the_documented_format() {
        for (name, base) in [
            ("Team Alpha", "team-alpha"),
            ("Les Pâtissiers", "les-patissiers"),
            ("CTF/2026", "ctf-2026"),
            ("Hexagon", "hexagon"),
        ] {
            let slug = generate_unique_slug(name, |_| async { Ok(false) })
                .await
                .expect("a slug must be generated");

            assert!(
                matches_slug_format(&slug, base, SUFFIX_LEN),
                "{slug:?} does not match ^{base}-[23456789a-hjkmnp-z]{{{SUFFIX_LEN}}}$"
            );
        }
    }
}
