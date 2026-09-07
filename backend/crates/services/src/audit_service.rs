use domain::{entities::instance_audit_logs, types::AuditAction};
use sea_orm::{ActiveModelTrait, ConnectionTrait, DbErr, Set, prelude::IpNetwork};
use thiserror::Error;
use uuid::Uuid;

/// Who performed the action and from where. `actor_id` is None for actions
/// taken by the CLI or by the system itself; ip and user_agent are None
/// outside the HTTP path.
pub struct AuditContext {
    pub actor_id: Option<Uuid>,
    pub ip_address: Option<IpNetwork>,
    pub user_agent: Option<String>,
}

/// What the action was aimed at. All three are optional: some actions target
/// nothing (a rate limit spike is tied to an IP, not to a row).
#[derive(Default)]
pub struct AuditTarget {
    pub user_id: Option<Uuid>,
    pub team_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
}

#[derive(Debug, Error)]
pub enum AuditError {
    #[error(transparent)]
    Db(#[from] DbErr),
}

/// Appends an entry. Generic over the connection so callers can write inside
/// the transaction that performs the action: an action that commits without
/// its audit entry, or the reverse, would leave the log lying.
pub async fn record<C: ConnectionTrait>(
    db: &C,
    action: AuditAction,
    ctx: &AuditContext,
    target: AuditTarget,
    reason: Option<&str>,
    meta: serde_json::Value,
) -> Result<(), AuditError> {
    // Insert into instance_audit_logs. `meta` holds a snapshot of the values
    // before the change, so the log stays readable once the row has moved on.
    // Never expose a delete path: the table is append only.
    instance_audit_logs::ActiveModel {
        action: Set(action.as_str().to_owned()),
        actor_id: Set(ctx.actor_id),
        ip_address: Set(ctx.ip_address),
        user_agent: Set(ctx.user_agent.clone()),
        target_user_id: Set(target.user_id),
        target_team_id: Set(target.team_id),
        target_workspace_id: Set(target.workspace_id),
        reason: Set(reason.map(|s| s.to_owned())),
        meta: Set(meta),
        ..Default::default()
    }
    .insert(db)
    .await
    .map_err(AuditError::Db)?;

    Ok(())
}
