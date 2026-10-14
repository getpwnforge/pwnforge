//! The two migrations of the team invitation rework, each at its own point
//! and each with its `down()`: every team invitation given an expiry, and
//! the `invited_by` columns on both membership tables.
//!
//! Each test works in a throwaway database of its own, migrated up to just
//! before the migration under test. Rows are written with statements: the
//! entities describe the schema after all three. CI provisions the instance
//! and sets `DATABASE_URL`; locally the dev compose exposes one.

use chrono::{DateTime, Duration, Utc};
use migration::MigratorTrait;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};
use uuid::Uuid;

#[path = "common/migrations.rs"]
mod migrations;
#[path = "common/scratch.rs"]
mod scratch;

use migrations::position_of;
use scratch::in_scratch_database;

const EXPIRE: &str = "m20261014_120000_team_invitations_expire";
const INVITED_BY: &str = "m20261014_121000_members_invited_by";

async fn migrate_to_just_before(db: &DatabaseConnection, name: &str) {
    migration::Migrator::up(db, Some(position_of(name)))
        .await
        .expect("the earlier migrations must apply");
}

async fn run(db: &DatabaseConnection, sql: &str, values: Vec<sea_orm::Value>) -> Result<(), DbErr> {
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        sql,
        values,
    ))
    .await
    .map(|_| ())
}

async fn create_user(db: &DatabaseConnection) -> Uuid {
    let id = Uuid::now_v7();
    run(
        db,
        "INSERT INTO users (id, username, password_hash, locale) VALUES ($1, $2, 'x', 'en')",
        vec![id.into(), format!("im-{}", id.simple()).into()],
    )
    .await
    .expect("the fixture user must insert");
    id
}

async fn create_team(db: &DatabaseConnection) -> Uuid {
    let id = Uuid::now_v7();
    run(
        db,
        "INSERT INTO teams (id, slug, name, color) VALUES ($1, $2, 'Migrations', 'ember')",
        vec![id.into(), format!("im-{}", id.simple()).into()],
    )
    .await
    .expect("the fixture team must insert");
    id
}

async fn team_invitation(
    db: &DatabaseConnection,
    team_id: Uuid,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
) -> Uuid {
    let id = Uuid::now_v7();
    run(
        db,
        "INSERT INTO team_invitations (id, team_id, token, default_role, created_at, expires_at) \
         VALUES ($1, $2, $3, 'member', $4, $5)",
        vec![
            id.into(),
            team_id.into(),
            format!("hash-{}", id.simple()).into(),
            created_at.fixed_offset().into(),
            expires_at.map(|at| at.fixed_offset()).into(),
        ],
    )
    .await
    .expect("the fixture invitation must insert");
    id
}

async fn expires_at_of(db: &DatabaseConnection, id: Uuid) -> Option<DateTime<Utc>> {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT expires_at FROM team_invitations WHERE id = $1",
        [id.into()],
    ))
    .await
    .expect("the lookup must succeed")
    .and_then(|row| {
        row.try_get::<Option<DateTime<chrono::FixedOffset>>>("", "expires_at")
            .expect("a timestamp")
    })
    .map(|at| at.with_timezone(&Utc))
}

async fn count(db: &DatabaseConnection, sql: &str) -> i64 {
    db.query_one_raw(Statement::from_string(DbBackend::Postgres, sql.to_owned()))
        .await
        .expect("the count must succeed")
        .and_then(|row| row.try_get::<i64>("", "n").ok())
        .expect("a count returns a row")
}

async fn column_exists(db: &DatabaseConnection, table: &str, column: &str) -> bool {
    db.query_one_raw(Statement::from_sql_and_values(
        DbBackend::Postgres,
        "SELECT 1 AS found FROM information_schema.columns \
         WHERE table_name = $1 AND column_name = $2",
        [table.into(), column.into()],
    ))
    .await
    .expect("the catalog query must succeed")
    .is_some()
}

fn close_to(actual: Option<DateTime<Utc>>, expected: DateTime<Utc>) -> bool {
    actual.is_some_and(|actual| (actual - expected).num_seconds().abs() < 60)
}

/// A row without expiry gets seven days from its creation, or seven days
/// from now when that is already past; one with an expiry keeps it. No row
/// is left without one, and the column refuses NULL from then on. `down()`
/// accepts NULL again and leaves the dates written.
#[tokio::test]
async fn every_team_invitation_gets_an_expiry_and_rolls_back() {
    in_scratch_database(|db| async move {
        migrate_to_just_before(&db, EXPIRE).await;
        let team = create_team(&db).await;
        let now = Utc::now();
        let recent = team_invitation(&db, team, now - Duration::days(2), None).await;
        let old = team_invitation(&db, team, now - Duration::days(30), None).await;
        let dated_at = now + Duration::days(20);
        let dated = team_invitation(&db, team, now, Some(dated_at)).await;

        migration::Migrator::up(&db, Some(1))
            .await
            .expect("the migration must apply");

        assert!(close_to(
            expires_at_of(&db, recent).await,
            now + Duration::days(5)
        ));
        assert!(
            close_to(expires_at_of(&db, old).await, now + Duration::days(7)),
            "an invitation already past seven days gets seven from now"
        );
        assert!(close_to(expires_at_of(&db, dated).await, dated_at));
        assert_eq!(
            count(
                &db,
                "SELECT count(*) AS n FROM team_invitations WHERE expires_at IS NULL"
            )
            .await,
            0,
            "no row is left without an expiry"
        );
        let refused = run(
            &db,
            "INSERT INTO team_invitations (id, team_id, token, default_role) \
             VALUES ($1, $2, 'hash-null', 'member')",
            vec![Uuid::now_v7().into(), team.into()],
        )
        .await;
        assert!(refused.is_err(), "the column must refuse NULL");

        migration::Migrator::down(&db, Some(1))
            .await
            .expect("down() must run");
        run(
            &db,
            "INSERT INTO team_invitations (id, team_id, token, default_role) \
             VALUES ($1, $2, 'hash-null', 'member')",
            vec![Uuid::now_v7().into(), team.into()],
        )
        .await
        .expect("after down() the column accepts NULL again");
        assert!(close_to(
            expires_at_of(&db, old).await,
            now + Duration::days(7)
        ));

        db.close().await.ok();
    })
    .await;
}

/// Both membership tables gain `invited_by`, NULL on the rows already there
/// and set to NULL when the author's account goes; `down()` drops both.
#[tokio::test]
async fn the_membership_tables_gain_invited_by_and_roll_back() {
    in_scratch_database(|db| async move {
        migrate_to_just_before(&db, INVITED_BY).await;
        let author = create_user(&db).await;
        let member = create_user(&db).await;
        let team = create_team(&db).await;
        run(
            &db,
            "INSERT INTO team_members (team_id, user_id, role) VALUES ($1, $2, 'member')",
            vec![team.into(), member.into()],
        )
        .await
        .expect("the fixture membership must insert");

        migration::Migrator::up(&db, Some(1))
            .await
            .expect("the migration must apply");
        for table in ["team_members", "workspace_members"] {
            assert!(column_exists(&db, table, "invited_by").await, "{table}");
        }
        assert_eq!(
            count(
                &db,
                "SELECT count(*) AS n FROM team_members WHERE invited_by IS NOT NULL"
            )
            .await,
            0,
            "existing rows record no author"
        );

        let newcomer = create_user(&db).await;
        run(
            &db,
            "INSERT INTO team_members (team_id, user_id, role, invited_by) \
             VALUES ($1, $2, 'member', $3)",
            vec![team.into(), newcomer.into(), author.into()],
        )
        .await
        .expect("a membership with its author inserts");
        run(&db, "DELETE FROM users WHERE id = $1", vec![author.into()])
            .await
            .expect("the author's account goes");
        assert_eq!(
            count(
                &db,
                "SELECT count(*) AS n FROM team_members WHERE invited_by IS NOT NULL"
            )
            .await,
            0,
            "the membership stays, its attribution goes"
        );
        assert_eq!(
            count(&db, "SELECT count(*) AS n FROM team_members").await,
            2
        );

        migration::Migrator::down(&db, Some(1))
            .await
            .expect("down() must run");
        for table in ["team_members", "workspace_members"] {
            assert!(!column_exists(&db, table, "invited_by").await, "{table}");
        }

        db.close().await.ok();
    })
    .await;
}
