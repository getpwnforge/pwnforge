//! Integration tests for `workspace_service::list_effective_members`.
//!
//! These need a real Postgres. What is under test is the merge of three
//! tables, and `MockDatabase` does not execute SQL: it replays whatever rows
//! the test hands it, so an override that should have shadowed a team role
//! would "pass" purely because the author remembered to shadow it by hand.
//! The same reasoning as `team_invitations.rs`, which has the longer note.
//!
//! CI provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one.
//!
//! These run in autocommit and leave their rows behind, same as the other
//! integration tests here.

use std::sync::atomic::{AtomicU32, Ordering};

use domain::dto::team::TeamRole;
use domain::dto::workspace::{EffectiveMember, Membership};
use domain::entities::{team_members, teams, users, workspace_members, workspaces};
use migration::MigratorTrait;
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, Set};
use services::workspace_service::{self, WorkspaceError};
use uuid::Uuid;

async fn connect() -> DatabaseConnection {
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://pwnforge:changeme@127.0.0.1:5432/pwnforge".to_owned());

    let db = Database::connect(url)
        .await
        .expect("a Postgres instance must be reachable (see the module docs)");

    migration::Migrator::up(&db, None)
        .await
        .expect("migrations must apply");

    db
}

/// A distinct suffix per call, so rows from one test never collide with
/// another's on the UNIQUE columns (`users.username`, `workspaces.slug`).
fn unique_suffix() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nth = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{}-{nth}", Uuid::now_v7().simple())
}

/// Returns the id and the username, since the username is what
/// `EffectiveMember::handle` carries and the assertions read better against
/// it than against a raw uuid.
async fn create_user(db: &DatabaseConnection) -> (Uuid, String) {
    create_named_user(db, &format!("ws-members-test-{}", unique_suffix())).await
}

/// A user with the display fields set, so the widened `EffectiveMember` is
/// asserted against something other than the empty-string fallback.
async fn create_named_user(db: &DatabaseConnection, username: &str) -> (Uuid, String) {
    let user = users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(username.to_owned()),
        password_hash: Set("not-a-real-hash".to_owned()),
        first_name: Set(Some("Ada".to_owned())),
        last_name: Set(Some("Lovelace".to_owned())),
        avatar_version: Set(Some(Uuid::now_v7())),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert");

    (user.id, user.username)
}

async fn create_team(db: &DatabaseConnection, creator: Uuid) -> Uuid {
    let suffix = unique_suffix();

    let team = teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(format!("ws-members-test-{suffix}")),
        name: Set(format!("Workspace Members Test {suffix}")),
        color: Set("ember".to_owned()),
        created_by: Set(Some(creator)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture team must insert");

    team.id
}

async fn add_team_member(db: &DatabaseConnection, team_id: Uuid, user_id: Uuid, role: TeamRole) {
    team_members::ActiveModel {
        team_id: Set(team_id),
        user_id: Set(user_id),
        role: Set(role.as_str().to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture team member must insert");
}

async fn create_workspace(
    db: &DatabaseConnection,
    team_id: Option<Uuid>,
    owner_id: Option<Uuid>,
) -> workspaces::Model {
    let suffix = unique_suffix();

    workspaces::ActiveModel {
        id: Set(Uuid::now_v7()),
        team_id: Set(team_id),
        owner_id: Set(owner_id),
        slug: Set(format!("ws-members-test-{suffix}")),
        name: Set(format!("Workspace Members Test {suffix}")),
        color: Set("ember".to_owned()),
        r#type: Set("event".to_owned()),
        mode: Set(if team_id.is_some() { "team" } else { "solo" }.to_owned()),
        default_writeup_visibility: Set("draft".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture workspace must insert")
}

/// `role: None` means an override row that sets no role, the shape a guest
/// may not have but a team member may.
async fn set_override(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    user_id: Uuid,
    role: Option<&str>,
    is_guest: bool,
) {
    workspace_members::ActiveModel {
        workspace_id: Set(workspace_id),
        user_id: Set(user_id),
        role: Set(role.map(str::to_owned)),
        is_guest: Set(is_guest),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture override must insert");
}

fn find(members: &[EffectiveMember], user_id: Uuid) -> Option<&EffectiveMember> {
    members.iter().find(|m| m.user_id == user_id)
}

/// Positive: a solo workspace lists exactly its owner, as `Owner`.
///
/// `Owner` and not `Active(Captain)`: it has to be the same answer
/// `permissions_service::effective_role` gives for that user, or the roster
/// and the caller's own membership would disagree about the same person.
#[tokio::test]
async fn solo_workspace_lists_its_owner_as_owner() {
    let db = connect().await;
    let (owner_id, owner_handle) = create_user(&db).await;
    let workspace = create_workspace(&db, None, Some(owner_id)).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a solo workspace must succeed");

    assert_eq!(members.len(), 1, "a solo workspace has exactly one member");
    assert_eq!(members[0].user_id, owner_id);
    assert_eq!(members[0].handle, owner_handle);
    assert_eq!(members[0].role, Membership::Owner);
    assert!(!members[0].is_guest);
}

/// Positive: team members with no override inherit the role they hold on the
/// team, each one their own.
#[tokio::test]
async fn team_members_inherit_their_team_role() {
    let db = connect().await;
    let (captain_id, captain_handle) = create_user(&db).await;
    let (viewer_id, _) = create_user(&db).await;
    let team_id = create_team(&db, captain_id).await;
    add_team_member(&db, team_id, captain_id, TeamRole::Captain).await;
    add_team_member(&db, team_id, viewer_id, TeamRole::Viewer).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    assert_eq!(members.len(), 2);

    let captain = find(&members, captain_id).expect("the captain must be listed");
    assert_eq!(captain.role, Membership::Active(TeamRole::Captain));
    assert_eq!(captain.handle, captain_handle);
    assert!(!captain.is_guest);

    let viewer = find(&members, viewer_id).expect("the viewer must be listed");
    assert_eq!(viewer.role, Membership::Active(TeamRole::Viewer));
    assert!(!viewer.is_guest);
}

/// Positive: an override role shadows the inherited team role, in both
/// directions. The demotion is the one that matters: if the override were
/// dropped, a Viewer on this workspace would be listed with the Admin rights
/// they hold on the team.
#[tokio::test]
async fn an_override_role_shadows_the_inherited_team_role() {
    let db = connect().await;
    let (promoted_id, _) = create_user(&db).await;
    let (demoted_id, _) = create_user(&db).await;
    let team_id = create_team(&db, promoted_id).await;
    add_team_member(&db, team_id, promoted_id, TeamRole::Viewer).await;
    add_team_member(&db, team_id, demoted_id, TeamRole::Admin).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;
    set_override(&db, workspace.id, promoted_id, Some("admin"), false).await;
    set_override(&db, workspace.id, demoted_id, Some("viewer"), false).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    assert_eq!(
        find(&members, promoted_id).map(|m| m.role),
        Some(Membership::Active(TeamRole::Admin)),
        "the override must win over the team role"
    );
    assert_eq!(
        find(&members, demoted_id).map(|m| m.role),
        Some(Membership::Active(TeamRole::Viewer)),
        "a demoting override must win over the team role too"
    );
}

/// Positive: an override row carrying no role overrides nothing and falls
/// through to the team role, rather than denying access just because the row
/// exists. Same fall-through as `permissions_service::effective_role`.
#[tokio::test]
async fn an_override_with_no_role_falls_through_to_the_team_role() {
    let db = connect().await;
    let (member_id, _) = create_user(&db).await;
    let team_id = create_team(&db, member_id).await;
    add_team_member(&db, team_id, member_id, TeamRole::Member).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;
    set_override(&db, workspace.id, member_id, None, false).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    assert_eq!(
        find(&members, member_id).map(|m| m.role),
        Some(Membership::Active(TeamRole::Member)),
        "a role-less override row must not change what the member inherits"
    );
}

/// Negative: an excluded team member is absent from the roster entirely, not
/// listed with some downgraded role. `EffectiveMember` has no way to say
/// "excluded", so leaking them here would put someone on a members screen who
/// cannot open the workspace at all.
#[tokio::test]
async fn an_excluded_team_member_is_not_listed() {
    let db = connect().await;
    let (kept_id, _) = create_user(&db).await;
    let (excluded_id, _) = create_user(&db).await;
    let team_id = create_team(&db, kept_id).await;
    add_team_member(&db, team_id, kept_id, TeamRole::Captain).await;
    add_team_member(&db, team_id, excluded_id, TeamRole::Admin).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;
    set_override(&db, workspace.id, excluded_id, Some("excluded"), false).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    assert_eq!(members.len(), 1);
    assert!(
        find(&members, excluded_id).is_none(),
        "an excluded member must not appear on the roster at all"
    );
    assert!(find(&members, kept_id).is_some());
}

/// Positive: a guest, who has no team membership behind them, is listed with
/// the role their override carries and flagged as a guest.
#[tokio::test]
async fn a_guest_is_listed_with_their_override_role() {
    let db = connect().await;
    let (captain_id, _) = create_user(&db).await;
    let (guest_id, guest_handle) = create_user(&db).await;
    let team_id = create_team(&db, captain_id).await;
    add_team_member(&db, team_id, captain_id, TeamRole::Captain).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;
    set_override(&db, workspace.id, guest_id, Some("member"), true).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    assert_eq!(members.len(), 2);

    let guest = find(&members, guest_id).expect("the guest must be listed");
    assert_eq!(guest.handle, guest_handle);
    assert_eq!(guest.role, Membership::Active(TeamRole::Member));
    assert!(guest.is_guest, "a guest must be flagged as one");

    let captain = find(&members, captain_id).expect("the team member must still be listed");
    assert!(
        !captain.is_guest,
        "a team member must not be flagged as a guest"
    );
}

/// Negative: a member of another team is not listed, so the roster is scoped
/// to this workspace's own team rather than to every team_members row.
#[tokio::test]
async fn a_member_of_another_team_is_not_listed() {
    let db = connect().await;
    let (ours_id, _) = create_user(&db).await;
    let (theirs_id, _) = create_user(&db).await;
    let our_team = create_team(&db, ours_id).await;
    let their_team = create_team(&db, theirs_id).await;
    add_team_member(&db, our_team, ours_id, TeamRole::Captain).await;
    add_team_member(&db, their_team, theirs_id, TeamRole::Captain).await;
    let workspace = create_workspace(&db, Some(our_team), None).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    assert_eq!(members.len(), 1);
    assert!(find(&members, theirs_id).is_none());
}

/// Negative: an override belonging to a different workspace of the same team
/// does not reach this one. Without the workspace filter, an exclusion on a
/// sibling workspace would silently remove someone here.
#[tokio::test]
async fn an_override_on_a_sibling_workspace_does_not_apply_here() {
    let db = connect().await;
    let (member_id, _) = create_user(&db).await;
    let team_id = create_team(&db, member_id).await;
    add_team_member(&db, team_id, member_id, TeamRole::Admin).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;
    let sibling = create_workspace(&db, Some(team_id), None).await;
    set_override(&db, sibling.id, member_id, Some("excluded"), false).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    assert_eq!(
        find(&members, member_id).map(|m| m.role),
        Some(Membership::Active(TeamRole::Admin)),
        "the sibling workspace's exclusion must not reach this roster"
    );
}

/// Positive: the display fields the members table needs come back on the same
/// read. `workspaces.members.columns` has `member` and `handle` as separate
/// columns, so a name and an avatar are part of the row, not a second fetch.
#[tokio::test]
async fn a_member_carries_the_display_fields_the_table_needs() {
    let db = connect().await;
    let (member_id, handle) = create_user(&db).await;
    let team_id = create_team(&db, member_id).await;
    add_team_member(&db, team_id, member_id, TeamRole::Admin).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    let member = find(&members, member_id).expect("the member must be listed");
    assert_eq!(member.handle, handle);
    assert_eq!(member.first_name, "Ada");
    assert_eq!(member.last_name, "Lovelace");
    assert!(
        member.avatar_version.is_some(),
        "the avatar version must be served from this read"
    );
}

/// Positive: a solo owner's `joined_at` is the workspace's own creation date.
/// Sole ownership has no membership row behind it, so there is no other date
/// to report, and leaving it unset would put an epoch timestamp in the
/// `joined` column.
#[tokio::test]
async fn a_solo_owner_joined_when_the_workspace_was_created() {
    let db = connect().await;
    let (owner_id, _) = create_user(&db).await;
    let workspace = create_workspace(&db, None, Some(owner_id)).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a solo workspace must succeed");

    assert_eq!(
        members[0].joined_at,
        chrono::DateTime::<chrono::Utc>::from(workspace.created_at)
    );
}

/// Positive: a guest's `joined_at` is their own override row's, not the
/// workspace's, since that is when they actually gained access.
#[tokio::test]
async fn a_guest_joined_when_their_override_row_was_written() {
    let db = connect().await;
    let (captain_id, _) = create_user(&db).await;
    let (guest_id, _) = create_user(&db).await;
    let team_id = create_team(&db, captain_id).await;
    add_team_member(&db, team_id, captain_id, TeamRole::Captain).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;
    set_override(&db, workspace.id, guest_id, Some("member"), true).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    let guest = find(&members, guest_id).expect("the guest must be listed");
    assert!(
        guest.joined_at > chrono::DateTime::<chrono::Utc>::from(workspace.created_at),
        "a guest's joined_at must be their own row's, which is later than \
         the workspace's creation"
    );
}

/// Positive: the roster comes back ordered, seniority first and handle within
/// a rank. Postgres makes no promise about the order rows come back in, so
/// without the sort the members table reshuffles between two refreshes that
/// read the same data.
#[tokio::test]
async fn the_roster_is_ordered_by_seniority_then_handle() {
    let db = connect().await;
    let suffix = unique_suffix();

    // Inserted in an order that is neither the expected one nor its reverse,
    // and with handles whose alphabetical order cuts across the roles, so
    // neither sort key could pass on its own: by handle alone the viewer
    // would lead, and by rank alone the two admins could come back either
    // way round.
    let (viewer_id, _) = create_named_user(&db, &format!("ws-sort-a-{suffix}")).await;
    let (admin_d_id, _) = create_named_user(&db, &format!("ws-sort-d-{suffix}")).await;
    let (captain_id, _) = create_named_user(&db, &format!("ws-sort-e-{suffix}")).await;
    let (admin_c_id, _) = create_named_user(&db, &format!("ws-sort-c-{suffix}")).await;

    let team_id = create_team(&db, captain_id).await;
    add_team_member(&db, team_id, viewer_id, TeamRole::Viewer).await;
    add_team_member(&db, team_id, admin_d_id, TeamRole::Admin).await;
    add_team_member(&db, team_id, captain_id, TeamRole::Captain).await;
    add_team_member(&db, team_id, admin_c_id, TeamRole::Admin).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    let order: Vec<Uuid> = members.iter().map(|m| m.user_id).collect();
    assert_eq!(
        order,
        vec![captain_id, admin_c_id, admin_d_id, viewer_id],
        "captain, then the two admins by handle, then the viewer"
    );
}

/// The ordering is case-insensitive, matching the CITEXT column the handles
/// come from: a byte ordering would file every capitalised handle ahead of
/// every lowercase one.
#[tokio::test]
async fn the_roster_orders_handles_case_insensitively() {
    let db = connect().await;
    let suffix = unique_suffix();

    let (zoe_id, _) = create_named_user(&db, &format!("ws-case-Zoe-{suffix}")).await;
    let (alice_id, _) = create_named_user(&db, &format!("ws-case-alice-{suffix}")).await;

    let team_id = create_team(&db, zoe_id).await;
    add_team_member(&db, team_id, zoe_id, TeamRole::Member).await;
    add_team_member(&db, team_id, alice_id, TeamRole::Member).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    let order: Vec<Uuid> = members.iter().map(|m| m.user_id).collect();
    assert_eq!(
        order,
        vec![alice_id, zoe_id],
        "alice must come before Zoe, as the CITEXT column orders them"
    );
}

/// The invariant that ties the two resolvers together: an excluded member is
/// absent from the roster *and* refused the workspace. Both halves come from
/// the same `workspace_members` row, but by two different code paths, and
/// either one drifting alone is silent.
///
/// Absent-but-admitted would show a members table missing someone who can
/// open the workspace. Listed-but-refused is the inverse: a row on a screen
/// for a person the product says has no access. `WorkspaceNotFound` and not
/// a permission error, because `get_workspace_for_member` must not confirm
/// the workspace exists to someone excluded from it
/// (docs/security-invariants.md, "A 404 hides existence, a 403 admits it").
#[tokio::test]
async fn an_excluded_member_is_both_unlisted_and_refused_the_workspace() {
    let db = connect().await;
    let (captain_id, _) = create_user(&db).await;
    let (excluded_id, _) = create_user(&db).await;
    let team_id = create_team(&db, captain_id).await;
    add_team_member(&db, team_id, captain_id, TeamRole::Captain).await;
    add_team_member(&db, team_id, excluded_id, TeamRole::Admin).await;
    let workspace = create_workspace(&db, Some(team_id), None).await;
    set_override(&db, workspace.id, excluded_id, Some("excluded"), false).await;

    let members = workspace_service::list_effective_members(&db, &workspace)
        .await
        .expect("listing a team workspace must succeed");

    assert!(
        find(&members, excluded_id).is_none(),
        "the excluded member must not be on the roster"
    );

    let refused =
        workspace_service::get_workspace_for_member(&db, &workspace.slug, excluded_id).await;

    assert!(
        matches!(refused, Err(WorkspaceError::WorkspaceNotFound)),
        "the excluded member must be refused the workspace as not-found, got {refused:?}"
    );

    // The other half of the invariant: the captain is on the roster, so the
    // test would fail if both halves had simply stopped working.
    assert!(find(&members, captain_id).is_some());
    assert!(
        workspace_service::get_workspace_for_member(&db, &workspace.slug, captain_id)
            .await
            .is_ok(),
        "the captain must still be admitted"
    );
}
