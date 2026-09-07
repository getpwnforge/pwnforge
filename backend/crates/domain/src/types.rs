// crates/domain/src/types.rs

#[derive(Debug, Clone, Copy)]
pub enum BlockSource {
    Manual,
    DisposableList,
}

impl BlockSource {
    pub fn as_str(self) -> &'static str {
        match self {
            BlockSource::Manual => "manual",
            BlockSource::DisposableList => "disposable-list",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ReservedSource {
    Manual,
    ReservedList,
}

impl ReservedSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ReservedSource::Manual => "manual",
            ReservedSource::ReservedList => "reserved-list",
        }
    }
}

pub enum RevokedReason {
    Rotated,
    UserLogout,
    ReuseDetected,
    PasswordReset,
    PasswordChange,
    AdminRevoked,
    AdminForcePasswordReset,
}

impl RevokedReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Rotated => "rotated",
            Self::UserLogout => "user_logout",
            Self::ReuseDetected => "reuse_detected",
            Self::PasswordReset => "password_reset",
            Self::PasswordChange => "password_change",
            Self::AdminRevoked => "admin_revoked",
            Self::AdminForcePasswordReset => "admin_force_password_reset",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    EmailVerification,
    PasswordReset,
}

impl TokenKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenKind::EmailVerification => "email_verification",
            TokenKind::PasswordReset => "password_reset",
        }
    }
}

/// Canonical action names for the instance audit log. Kept as an enum rather
/// than free strings so a typo cannot silently create a new action nobody
/// queries for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditAction {
    UserSuspend,
    UserUnsuspend,
    UserDeleteByAdmin,
    UserDeletionScheduled,
    UserPurged,
    UserDeletionCancelled,
    UserPromoteAdmin,
    UserDemoteAdmin,
    UserPasswordResetForce,
    UserSessionsRevokeAll,
    AuthRefreshReuseDetected,
    SignupDisposableBlocked,
    SignupRateLimited,
    InstanceSetupCompleted,
    AlertCreated,
    AlertUpdated,
    AlertDeleted,
}

impl AuditAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserSuspend => "user.suspend",
            Self::UserUnsuspend => "user.unsuspend",
            Self::UserDeleteByAdmin => "user.delete_by_admin",
            Self::UserDeletionScheduled => "user.deletion_scheduled",
            Self::UserPurged => "user.purged",
            Self::UserDeletionCancelled => "user.deletion_cancelled",
            Self::UserPromoteAdmin => "user.promote_admin",
            Self::UserDemoteAdmin => "user.demote_admin",
            Self::UserPasswordResetForce => "user.password_reset_force",
            Self::UserSessionsRevokeAll => "user.sessions_revoke_all",
            Self::AuthRefreshReuseDetected => "auth.refresh_reuse_detected",
            Self::SignupDisposableBlocked => "signup.disposable_blocked",
            Self::SignupRateLimited => "signup.rate_limited",
            Self::InstanceSetupCompleted => "instance.setup_completed",
            Self::AlertCreated => "alert.created",
            Self::AlertUpdated => "alert.updated",
            Self::AlertDeleted => "alert.deleted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Info,
    Warning,
    Danger,
    Maintenance,
}

impl AlertKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AlertKind::Info => "info",
            AlertKind::Warning => "warning",
            AlertKind::Danger => "danger",
            AlertKind::Maintenance => "maintenance",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "info" => Some(AlertKind::Info),
            "warning" => Some(AlertKind::Warning),
            "danger" => Some(AlertKind::Danger),
            "maintenance" => Some(AlertKind::Maintenance),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContactCategory {
    General,
    SelfHosting,
    Billing,
    Press,
    Other,
}

impl ContactCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::General => "General question",
            Self::SelfHosting => "Self-hosting help",
            Self::Billing => "Billing & sales",
            Self::Press => "Press & partnerships",
            Self::Other => "Other",
        }
    }
}
