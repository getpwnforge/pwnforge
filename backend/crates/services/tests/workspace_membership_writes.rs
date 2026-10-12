//! Integration tests for the writes that create and destroy workspace
//! membership overrides: `workspace_service::set_member_override`,
//! `exclude_member`, `add_favorite`, and the cleanup
//! `team_service::remove_member` performs.
//!
//! These need a real Postgres. What is under test is a transaction, a
//! `DELETE ... WHERE` and a unique constraint, none of which `MockDatabase`
//! executes: it replays the rows the test hands it, so a cleanup that deleted
//! nothing would still "pass". Same reasoning as `team_invitations.rs`, which
//! has the longer note.
//!
//! CI provisions the instance and sets `DATABASE_URL`; locally the dev compose
//! exposes one.

use std::sync::atomic::{AtomicU32, Ordering};

use domain::dto::team::TeamRole;
use domain::dto::workspace::Membership;
use domain::entities::{
    team_members, teams, users, workspace_favorites, workspace_members, workspaces,
};
use migration::MigratorTrait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter, Set,
};
use services::workspace_service::WorkspaceError;
use services::{permissions_service, team_service, workspace_service};
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

fn unique_suffix() -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nth = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("{}-{nth}", Uuid::now_v7().simple())
}

async fn create_user(db: &DatabaseConnection) -> Uuid {
    users::ActiveModel {
        id: Set(Uuid::now_v7()),
        username: Set(format!("ws-writes-test-{}", unique_suffix())),
        password_hash: Set("not-a-real-hash".to_owned()),
        locale: Set("en".to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture user must insert")
    .id
}

async fn create_team(db: &DatabaseConnection, creator: Uuid) -> Uuid {
    let suffix = unique_suffix();

    teams::ActiveModel {
        id: Set(Uuid::now_v7()),
        slug: Set(format!("ws-writes-test-{suffix}")),
        name: Set(format!("Workspace Writes Test {suffix}")),
        color: Set("ember".to_owned()),
        created_by: Set(Some(creator)),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("the fixture team must insert")
    .id
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
        slug: Set(format!("ws-writes-test-{suffix}")),
        name: Set(format!("Workspace Writes Test {suffix}")),
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

async fn override_row(
    db: &DatabaseConnection,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Option<workspace_members::Model> {
    workspace_members::Entity::find_by_id((workspace_id, user_id))
        .one(db)
        .await
        .expect("the override lookup must succeed")
}

/// A team with one captain, one ordinary member, and one workspace.
struct Fixture {
    team_id: Uuid,
    captain_id: Uuid,
    member_id: Uuid,
    workspace: workspaces::Model,
}

async fn fixture(db: &DatabaseConnection) -> Fixture {
    let captain_id = create_user(db).await;
    let member_id = create_user(db).await;
    let team_id = create_team(db, captain_id).await;
    add_team_member(db, team_id, captain_id, TeamRole::Captain).await;
    add_team_member(db, team_id, member_id, TeamRole::Member).await;
    let workspace = create_workspace(db, Some(team_id), None).await;

    Fixture {
        team_id,
        captain_id,
        member_id,
        workspace,
    }
}

// ---- remove_member cleanup ------------------------------------------------

/// The invariant this lot exists for: leaving a team takes the person's
/// workspace overrides with it.
///
/// Both halves are asserted, because the row surviving is not a cosmetic
/// leftover. `effective_role` reads the override before team membership, so a
/// surviving row keeps admitting someone who is no longer on the team, while
/// `list_effective_members` drops them from the roster for exactly the same
/// reason. Access with no place on the members list is the state this
/// prevents.
#[tokio::test]
async fn leaving_a_team_takes_the_workspace_overrides_with_it() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.member_id,
        TeamRole::Admin,
        false,
    )
    .await
    .expect("the override must be written");

    team_service::remove_member(&db, f.team_id, f.member_id)
        .await
        .expect("removing the member must succeed");

    assert!(
        override_row(&db, f.workspace.id, f.member_id)
            .await
            .is_none(),
        "the override row must not outlive the team membership"
    );

    assert_eq!(
        permissions_service::effective_role(&db, f.member_id, &f.workspace)
            .await
            .expect("resolving the role must succeed"),
        Membership::None,
        "the removed member must have no access left"
    );
}

/// An exclusion goes too. It subtracts from inherited access, so with the
/// inheritance gone it names nothing; keeping it would silently deny the
/// workspace to whoever rejoined the team later, with nothing left to say why.
#[tokio::test]
async fn leaving_a_team_also_clears_an_exclusion() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::exclude_member(&db, f.workspace.id, f.member_id)
        .await
        .expect("the exclusion must be written");

    team_service::remove_member(&db, f.team_id, f.member_id)
        .await
        .expect("removing the member must succeed");

    assert!(
        override_row(&db, f.workspace.id, f.member_id)
            .await
            .is_none(),
        "the exclusion must not outlive the team membership"
    );

    // Rejoining gets the inherited access back, rather than a stale denial.
    add_team_member(&db, f.team_id, f.member_id, TeamRole::Member).await;

    assert_eq!(
        permissions_service::effective_role(&db, f.member_id, &f.workspace)
            .await
            .expect("resolving the role must succeed"),
        Membership::Active(TeamRole::Member),
        "rejoining must restore inherited access, not resurrect the exclusion"
    );
}

/// The cleanup is scoped to the team being left: overrides on another team's
/// workspaces survive, since that membership is untouched.
#[tokio::test]
async fn leaving_one_team_leaves_another_teams_overrides_alone() {
    let db = connect().await;
    let f = fixture(&db).await;

    let other_captain = create_user(&db).await;
    let other_team = create_team(&db, other_captain).await;
    add_team_member(&db, other_team, other_captain, TeamRole::Captain).await;
    add_team_member(&db, other_team, f.member_id, TeamRole::Member).await;
    let other_workspace = create_workspace(&db, Some(other_team), None).await;

    workspace_service::set_member_override(
        &db,
        other_workspace.id,
        f.member_id,
        TeamRole::Admin,
        false,
    )
    .await
    .expect("the override must be written");

    team_service::remove_member(&db, f.team_id, f.member_id)
        .await
        .expect("removing the member must succeed");

    assert!(
        override_row(&db, other_workspace.id, f.member_id)
            .await
            .is_some(),
        "an override on another team's workspace must survive"
    );
}

/// The cleanup is scoped to the person leaving: everyone else's overrides on
/// the same workspace survive.
#[tokio::test]
async fn leaving_a_team_leaves_other_peoples_overrides_alone() {
    let db = connect().await;
    let f = fixture(&db).await;

    let bystander_id = create_user(&db).await;
    add_team_member(&db, f.team_id, bystander_id, TeamRole::Member).await;

    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.member_id,
        TeamRole::Admin,
        false,
    )
    .await
    .expect("the override must be written");
    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        bystander_id,
        TeamRole::Viewer,
        false,
    )
    .await
    .expect("the bystander's override must be written");

    team_service::remove_member(&db, f.team_id, f.member_id)
        .await
        .expect("removing the member must succeed");

    assert!(
        override_row(&db, f.workspace.id, bystander_id)
            .await
            .is_some(),
        "another member's override on the same workspace must survive"
    );
}

/// A guest has no team membership to lose, so removing someone else from the
/// team must not touch them.
#[tokio::test]
async fn leaving_a_team_leaves_guests_alone() {
    let db = connect().await;
    let f = fixture(&db).await;

    let guest_id = create_user(&db).await;
    workspace_service::set_member_override(&db, f.workspace.id, guest_id, TeamRole::Member, true)
        .await
        .expect("the guest must be admitted");

    team_service::remove_member(&db, f.team_id, f.member_id)
        .await
        .expect("removing the member must succeed");

    assert!(
        override_row(&db, f.workspace.id, guest_id).await.is_some(),
        "a guest must survive someone else leaving the team"
    );
}

/// The refusal still comes first: the Captain is not removed, so their
/// override is not cleaned up either. Adding a second delete to this path
/// must not turn a refusal into a partial write.
///
/// This does not demonstrate the transaction. The Captain check runs before
/// either delete, so this test passes whether the two deletes share one or
/// not. What the transaction protects against is a failure *between* them,
/// which needs fault injection to reach and has no test here.
#[tokio::test]
async fn a_refused_removal_cleans_up_nothing() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.captain_id,
        TeamRole::Viewer,
        false,
    )
    .await
    .expect("the override must be written");

    let refused = team_service::remove_member(&db, f.team_id, f.captain_id).await;

    assert!(refused.is_err(), "the Captain must not be removable");
    assert!(
        override_row(&db, f.workspace.id, f.captain_id)
            .await
            .is_some(),
        "a refused removal must leave the override in place"
    );
}

/// A guest grant predates the team membership and outlives it.
///
/// The sequence is reachable and legitimate: admitted as a guest, later
/// invited onto the team, later removed from it. Deleting the guest row on the
/// way out would revoke a grant that leaving the team never asked to touch,
/// and silently: an operator can see and remove a guest row, but cannot see
/// one that is gone. So the cleanup is bounded to `is_guest = false`, which is
/// exactly the shape the orphan takes, so the guarantee is unaffected.
#[tokio::test]
async fn leaving_a_team_keeps_a_guest_grant_that_predates_it() {
    let db = connect().await;
    let f = fixture(&db).await;
    let guest_id = create_user(&db).await;

    // Admitted as a guest first, with no team membership behind it.
    workspace_service::set_member_override(&db, f.workspace.id, guest_id, TeamRole::Member, true)
        .await
        .expect("the guest must be admitted");

    // Then joins the team, and later leaves it.
    add_team_member(&db, f.team_id, guest_id, TeamRole::Viewer).await;
    team_service::remove_member(&db, f.team_id, guest_id)
        .await
        .expect("removing the member must succeed");

    let row = override_row(&db, f.workspace.id, guest_id)
        .await
        .expect("the guest row must survive the team stint");
    assert!(row.is_guest);

    assert_eq!(
        permissions_service::effective_role(&db, guest_id, &f.workspace)
            .await
            .expect("resolving the role must succeed"),
        Membership::Active(TeamRole::Member),
        "they must come out with the guest access they walked in with"
    );

    let members = workspace_service::list_effective_members(&db, &f.workspace)
        .await
        .expect("listing must succeed");
    let listed = members
        .iter()
        .find(|m| m.user_id == guest_id)
        .expect("they must be back on the roster as a guest");
    assert!(listed.is_guest, "and be shown as the guest they are again");
}

/// The bound does not weaken the orphan guarantee: the row the cleanup exists
/// to delete is a non-guest override, and it still goes.
#[tokio::test]
async fn the_guest_bound_still_deletes_a_non_guest_override() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.member_id,
        TeamRole::Admin,
        false,
    )
    .await
    .expect("the override must be written");

    team_service::remove_member(&db, f.team_id, f.member_id)
        .await
        .expect("removing the member must succeed");

    assert!(
        override_row(&db, f.workspace.id, f.member_id)
            .await
            .is_none(),
        "a non-guest override must still be deleted"
    );
}

// ---- set_member_override --------------------------------------------------

/// Positive: a team member gets a workspace role that shadows their team one.
#[tokio::test]
async fn an_override_can_be_written_for_a_team_member() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.member_id,
        TeamRole::Admin,
        false,
    )
    .await
    .expect("the override must be written");

    let row = override_row(&db, f.workspace.id, f.member_id)
        .await
        .expect("the row must exist");
    assert_eq!(row.role.as_deref(), Some("admin"));
    assert!(!row.is_guest);

    assert_eq!(
        permissions_service::effective_role(&db, f.member_id, &f.workspace)
            .await
            .expect("resolving the role must succeed"),
        Membership::Active(TeamRole::Admin),
    );
}

/// Positive: writing twice updates rather than failing, and does not move the
/// date the person gained access.
#[tokio::test]
async fn writing_an_override_twice_updates_it_and_keeps_joined_at() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.member_id,
        TeamRole::Admin,
        false,
    )
    .await
    .expect("the first write must succeed");
    let first = override_row(&db, f.workspace.id, f.member_id)
        .await
        .expect("the row must exist");

    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.member_id,
        TeamRole::Viewer,
        false,
    )
    .await
    .expect("the second write must succeed");
    let second = override_row(&db, f.workspace.id, f.member_id)
        .await
        .expect("the row must still exist");

    assert_eq!(second.role.as_deref(), Some("viewer"));
    assert_eq!(
        second.joined_at, first.joined_at,
        "changing what someone may do is not them arriving again"
    );
}

/// Positive: a guest is someone with no team membership, and is admitted as
/// one.
#[tokio::test]
async fn a_guest_can_be_admitted_without_team_membership() {
    let db = connect().await;
    let f = fixture(&db).await;
    let guest_id = create_user(&db).await;

    workspace_service::set_member_override(&db, f.workspace.id, guest_id, TeamRole::Member, true)
        .await
        .expect("the guest must be admitted");

    let row = override_row(&db, f.workspace.id, guest_id)
        .await
        .expect("the row must exist");
    assert!(row.is_guest);
    assert_eq!(row.role.as_deref(), Some("member"));
}

/// The creation-side guard. Without it, removal's cleanup closes the source
/// of orphan rows and this reopens it: the row would be written directly, and
/// `effective_role` would honour it while the roster showed nobody.
#[tokio::test]
async fn a_non_guest_override_is_refused_for_someone_outside_the_team() {
    let db = connect().await;
    let f = fixture(&db).await;
    let outsider_id = create_user(&db).await;

    let refused = workspace_service::set_member_override(
        &db,
        f.workspace.id,
        outsider_id,
        TeamRole::Admin,
        false,
    )
    .await;

    assert!(
        matches!(refused, Err(WorkspaceError::TargetNotInTeam)),
        "expected TargetNotInTeam, got {refused:?}"
    );
    assert!(
        override_row(&db, f.workspace.id, outsider_id)
            .await
            .is_none(),
        "the refused write must leave no row behind"
    );
}

/// The mirror guard: a team member is not a guest, and a row claiming so
/// would be a fact `list_effective_members` then contradicts.
#[tokio::test]
async fn a_guest_override_is_refused_for_a_team_member() {
    let db = connect().await;
    let f = fixture(&db).await;

    let refused = workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.member_id,
        TeamRole::Member,
        true,
    )
    .await;

    assert!(
        matches!(refused, Err(WorkspaceError::TargetAlreadyInTeam)),
        "expected TargetAlreadyInTeam, got {refused:?}"
    );
}

/// A row on a solo workspace is inert: neither resolver reads the table for
/// one. Refused rather than written and silently ignored.
#[tokio::test]
async fn an_override_is_refused_on_a_solo_workspace() {
    let db = connect().await;
    let owner_id = create_user(&db).await;
    let other_id = create_user(&db).await;
    let workspace = create_workspace(&db, None, Some(owner_id)).await;

    let refused =
        workspace_service::set_member_override(&db, workspace.id, other_id, TeamRole::Member, true)
            .await;

    assert!(
        matches!(refused, Err(WorkspaceError::SoloWorkspace)),
        "expected SoloWorkspace, got {refused:?}"
    );
}

/// An unknown workspace is not-found, not a row written against nothing.
#[tokio::test]
async fn an_override_on_an_unknown_workspace_is_not_found() {
    let db = connect().await;
    let user_id = create_user(&db).await;

    let refused = workspace_service::set_member_override(
        &db,
        Uuid::now_v7(),
        user_id,
        TeamRole::Member,
        false,
    )
    .await;

    assert!(
        matches!(refused, Err(WorkspaceError::WorkspaceNotFound)),
        "expected WorkspaceNotFound, got {refused:?}"
    );
}

// ---- exclude_member -------------------------------------------------------

/// Positive: exclusion denies the workspace while leaving team membership
/// intact, and both resolvers agree about it.
#[tokio::test]
async fn excluding_a_member_denies_the_workspace_but_keeps_the_team() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::exclude_member(&db, f.workspace.id, f.member_id)
        .await
        .expect("the exclusion must be written");

    assert_eq!(
        permissions_service::effective_role(&db, f.member_id, &f.workspace)
            .await
            .expect("resolving the role must succeed"),
        Membership::Excluded,
    );

    let members = workspace_service::list_effective_members(&db, &f.workspace)
        .await
        .expect("listing must succeed");
    assert!(
        !members.iter().any(|m| m.user_id == f.member_id),
        "an excluded member must not be on the roster"
    );

    assert!(
        team_members::Entity::find_by_id((f.team_id, f.member_id))
            .one(&db)
            .await
            .expect("the lookup must succeed")
            .is_some(),
        "exclusion must not touch team membership"
    );
}

/// Exclusion only means something against inherited access, so it is refused
/// for someone with none. A guest is dropped by deleting their row instead.
#[tokio::test]
async fn excluding_someone_outside_the_team_is_refused() {
    let db = connect().await;
    let f = fixture(&db).await;
    let outsider_id = create_user(&db).await;

    let refused = workspace_service::exclude_member(&db, f.workspace.id, outsider_id).await;

    assert!(
        matches!(refused, Err(WorkspaceError::TargetNotInTeam)),
        "expected TargetNotInTeam, got {refused:?}"
    );
}

/// An exclusion replaces a granted override rather than sitting beside it:
/// one row per person per workspace, so the last write is what holds.
#[tokio::test]
async fn excluding_replaces_an_existing_granted_override() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.member_id,
        TeamRole::Admin,
        false,
    )
    .await
    .expect("the grant must be written");
    workspace_service::exclude_member(&db, f.workspace.id, f.member_id)
        .await
        .expect("the exclusion must be written");

    let row = override_row(&db, f.workspace.id, f.member_id)
        .await
        .expect("the row must exist");
    assert_eq!(row.role.as_deref(), Some(Membership::Excluded.as_str()));

    assert_eq!(
        permissions_service::effective_role(&db, f.member_id, &f.workspace)
            .await
            .expect("resolving the role must succeed"),
        Membership::Excluded,
    );
}

// ---- remove_member_override -----------------------------------------------

/// On a team member, removing the override is a reset: they fall back to the
/// role they inherit from the team, not to no access.
#[tokio::test]
async fn removing_a_granted_override_falls_back_to_the_team_role() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.member_id,
        TeamRole::Admin,
        false,
    )
    .await
    .expect("the grant must be written");
    workspace_service::remove_member_override(&db, f.workspace.id, f.member_id)
        .await
        .expect("removing the override must succeed");

    assert!(
        override_row(&db, f.workspace.id, f.member_id)
            .await
            .is_none()
    );
    assert_eq!(
        permissions_service::effective_role(&db, f.member_id, &f.workspace)
            .await
            .expect("resolving the role must succeed"),
        Membership::Active(TeamRole::Member),
        "the inherited team role must come back, not no access at all"
    );
}

/// On an excluded team member, the same call *grants*: the inherited access
/// returns. This is the outcome most likely to surprise whoever wires a single
/// "remove" button in 3.9.
#[tokio::test]
async fn removing_an_exclusion_restores_inherited_access() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::exclude_member(&db, f.workspace.id, f.member_id)
        .await
        .expect("the exclusion must be written");
    workspace_service::remove_member_override(&db, f.workspace.id, f.member_id)
        .await
        .expect("removing the exclusion must succeed");

    assert_eq!(
        permissions_service::effective_role(&db, f.member_id, &f.workspace)
            .await
            .expect("resolving the role must succeed"),
        Membership::Active(TeamRole::Member),
        "lifting an exclusion must restore what the team grants"
    );

    let members = workspace_service::list_effective_members(&db, &f.workspace)
        .await
        .expect("listing must succeed");
    assert!(
        members.iter().any(|m| m.user_id == f.member_id),
        "and must put them back on the roster"
    );
}

/// On a guest the row was the whole of their access, so the same call revokes
/// it. Nothing falls back, because there is no team membership behind them.
#[tokio::test]
async fn removing_a_guests_override_revokes_their_access() {
    let db = connect().await;
    let f = fixture(&db).await;
    let guest_id = create_user(&db).await;

    workspace_service::set_member_override(&db, f.workspace.id, guest_id, TeamRole::Member, true)
        .await
        .expect("the guest must be admitted");
    workspace_service::remove_member_override(&db, f.workspace.id, guest_id)
        .await
        .expect("removing the guest must succeed");

    assert_eq!(
        permissions_service::effective_role(&db, guest_id, &f.workspace)
            .await
            .expect("resolving the role must succeed"),
        Membership::None,
        "a guest has nothing to fall back to, so this revokes"
    );

    let members = workspace_service::list_effective_members(&db, &f.workspace)
        .await
        .expect("listing must succeed");
    assert!(!members.iter().any(|m| m.user_id == guest_id));
}

/// No row is success, not `WorkspaceNotFound`: the caller asked for the
/// override to be gone and it is.
#[tokio::test]
async fn removing_an_override_that_is_not_there_succeeds() {
    let db = connect().await;
    let f = fixture(&db).await;

    workspace_service::remove_member_override(&db, f.workspace.id, f.member_id)
        .await
        .expect("removing an absent override must succeed");

    // Twice, so idempotence is pinned rather than inferred from one call.
    workspace_service::remove_member_override(&db, f.workspace.id, f.member_id)
        .await
        .expect("and must stay idempotent");
}

/// Scoped to the one row the composite key names: the same person's override
/// on a sibling workspace survives, and so does another person's on this one.
#[tokio::test]
async fn removing_an_override_touches_only_that_workspace_and_person() {
    let db = connect().await;
    let f = fixture(&db).await;
    let sibling = create_workspace(&db, Some(f.team_id), None).await;

    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.member_id,
        TeamRole::Admin,
        false,
    )
    .await
    .expect("the grant must be written");
    workspace_service::set_member_override(&db, sibling.id, f.member_id, TeamRole::Viewer, false)
        .await
        .expect("the sibling grant must be written");
    workspace_service::set_member_override(
        &db,
        f.workspace.id,
        f.captain_id,
        TeamRole::Viewer,
        false,
    )
    .await
    .expect("the captain's grant must be written");

    workspace_service::remove_member_override(&db, f.workspace.id, f.member_id)
        .await
        .expect("removing the override must succeed");

    assert!(
        override_row(&db, sibling.id, f.member_id).await.is_some(),
        "the same person's override on another workspace must survive"
    );
    assert!(
        override_row(&db, f.workspace.id, f.captain_id)
            .await
            .is_some(),
        "another person's override on this workspace must survive"
    );
}

// ---- add_favorite ---------------------------------------------------------

/// Favouriting twice is the same wish stated twice. Before this, the second
/// call surfaced the unique violation as an error, which the 3.9 route would
/// have turned into a failed request for a button the user had simply pressed
/// again.
#[tokio::test]
async fn favouriting_twice_is_a_no_op_success() {
    let db = connect().await;
    let user_id = create_user(&db).await;
    let workspace = create_workspace(&db, None, Some(user_id)).await;

    workspace_service::add_favorite(&db, user_id, workspace.id)
        .await
        .expect("the first favourite must succeed");
    workspace_service::add_favorite(&db, user_id, workspace.id)
        .await
        .expect("favouriting again must succeed rather than error");

    let count = workspace_favorites::Entity::find()
        .filter(workspace_favorites::Column::UserId.eq(user_id))
        .filter(workspace_favorites::Column::WorkspaceId.eq(workspace.id))
        .all(&db)
        .await
        .expect("the lookup must succeed")
        .len();

    assert_eq!(count, 1, "the second call must not add a second row");
}

/// The swallowed error is the duplicate and nothing else: a favourite naming
/// a workspace that does not exist still fails, on the foreign key.
#[tokio::test]
async fn favouriting_an_unknown_workspace_still_fails() {
    let db = connect().await;
    let user_id = create_user(&db).await;

    let refused = workspace_service::add_favorite(&db, user_id, Uuid::now_v7()).await;

    assert!(
        refused.is_err(),
        "a favourite on a missing workspace must not be swallowed as a duplicate"
    );
}
