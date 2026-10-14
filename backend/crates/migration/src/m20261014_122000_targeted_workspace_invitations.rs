//! `workspace_invitations` rebuilt for targeted invitations only. The table
//! from `m20260917_163621_workspaces` was shaped for shareable links
//! (`max_uses`, `uses`, `as_guest`) that nothing ever wrote; a workspace
//! invitation now names one account or one address, and keeps its history.
//!
//! A row is `pending` until it is accepted, declined, cancelled (by a member,
//! or because the workspace was archived) or expired, and then records when.
//! The address is kept only while the row is pending: the constraints refuse
//! it on any other status. The token is stored hashed.
//!
//! The previous table is dropped, not converted: a row of it has no target
//! to convert to. The migration refuses to run if it holds any, and nothing
//! wrote one. `down()` restores the previous table, empty.

use sea_orm_migration::prelude::*;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20261014_122000_targeted_workspace_invitations"
    }
}

const CREATE_TARGETED: &str = "
CREATE TABLE workspace_invitations (
    id uuid NOT NULL DEFAULT uuidv7() PRIMARY KEY,
    workspace_id uuid NOT NULL,
    invited_user_id uuid,
    invited_email citext,
    role text NOT NULL,
    token text NOT NULL UNIQUE,
    invited_by uuid,
    status text NOT NULL DEFAULT 'pending',
    cancel_reason text,
    accepted_by uuid,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    resolved_at timestamptz,
    CONSTRAINT fk_workspace_invitations_workspace_id FOREIGN KEY (workspace_id)
        REFERENCES workspaces (id) ON DELETE CASCADE,
    CONSTRAINT fk_workspace_invitations_invited_user_id FOREIGN KEY (invited_user_id)
        REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT fk_workspace_invitations_invited_by FOREIGN KEY (invited_by)
        REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT fk_workspace_invitations_accepted_by FOREIGN KEY (accepted_by)
        REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT ck_workspace_invitations_role CHECK (role IN ('member', 'viewer')),
    CONSTRAINT ck_workspace_invitations_status
        CHECK (status IN ('pending', 'accepted', 'declined', 'cancelled', 'expired')),
    CONSTRAINT ck_workspace_invitations_single_target
        CHECK (invited_user_id IS NULL OR invited_email IS NULL),
    CONSTRAINT ck_workspace_invitations_pending_has_target
        CHECK (status <> 'pending' OR invited_user_id IS NOT NULL OR invited_email IS NOT NULL),
    CONSTRAINT ck_workspace_invitations_email_while_pending
        CHECK (invited_email IS NULL OR status = 'pending'),
    CONSTRAINT ck_workspace_invitations_resolved_once_closed
        CHECK ((status = 'pending') = (resolved_at IS NULL)),
    CONSTRAINT ck_workspace_invitations_cancel_reason
        CHECK ((status = 'cancelled') = (cancel_reason IS NOT NULL)
            AND (cancel_reason IS NULL OR cancel_reason IN ('revoked', 'workspace_archived'))),
    CONSTRAINT ck_workspace_invitations_accepted_by
        CHECK (accepted_by IS NULL OR status = 'accepted')
)";

/// One pending invitation per account or address in a workspace. Partial on
/// `pending`, so the history keeps as many closed ones as there were.
const UNIQUE_PENDING: [&str; 2] = [
    "CREATE UNIQUE INDEX idx_workspace_invitations_pending_per_user \
     ON workspace_invitations (workspace_id, invited_user_id) \
     WHERE status = 'pending' AND invited_user_id IS NOT NULL",
    "CREATE UNIQUE INDEX idx_workspace_invitations_pending_per_email \
     ON workspace_invitations (workspace_id, invited_email) \
     WHERE status = 'pending' AND invited_email IS NOT NULL",
];

/// The table as `m20260917_163621_workspaces` created it.
const CREATE_PREVIOUS: &str = "
CREATE TABLE workspace_invitations (
    id uuid NOT NULL DEFAULT uuidv7() PRIMARY KEY,
    workspace_id uuid NOT NULL,
    token text NOT NULL UNIQUE,
    default_role text NOT NULL,
    created_by uuid,
    expires_at timestamptz NOT NULL,
    max_uses integer,
    uses integer NOT NULL DEFAULT 0,
    as_guest boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT fk_workspace_invitations_workspace_id FOREIGN KEY (workspace_id)
        REFERENCES workspaces (id) ON DELETE CASCADE,
    CONSTRAINT fk_workspace_invitations_created_by FOREIGN KEY (created_by)
        REFERENCES users (id) ON DELETE SET NULL
)";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        let rows = db
            .query_one_raw(sea_orm_migration::sea_orm::Statement::from_string(
                manager.get_database_backend(),
                "SELECT count(*) FROM workspace_invitations".to_owned(),
            ))
            .await?
            .ok_or_else(|| DbErr::Migration("a count returned no row".to_owned()))?
            .try_get_by_index::<i64>(0)?;
        if rows > 0 {
            return Err(DbErr::Migration(format!(
                "workspace_invitations holds {rows} row(s) of the previous shape, which name \
                 no target to convert to. Nothing in the application writes them: remove \
                 them by hand, then run the migration again."
            )));
        }

        db.execute_unprepared("DROP TABLE workspace_invitations")
            .await?;
        db.execute_unprepared(CREATE_TARGETED).await?;
        for index in UNIQUE_PENDING {
            db.execute_unprepared(index).await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TABLE workspace_invitations")
            .await?;
        db.execute_unprepared(CREATE_PREVIOUS).await?;
        Ok(())
    }
}
