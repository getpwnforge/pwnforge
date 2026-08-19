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
    UserPromoteAdmin,
    UserDemoteAdmin,
    UserPasswordResetForce,
    UserSessionsRevokeAll,
    AuthRefreshReuseDetected,
    SignupDisposableBlocked,
    SignupRateLimited,
    InstanceSetupCompleted,
}

impl AuditAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserSuspend => "user.suspend",
            Self::UserUnsuspend => "user.unsuspend",
            Self::UserDeleteByAdmin => "user.delete_by_admin",
            Self::UserPromoteAdmin => "user.promote_admin",
            Self::UserDemoteAdmin => "user.demote_admin",
            Self::UserPasswordResetForce => "user.password_reset_force",
            Self::UserSessionsRevokeAll => "user.sessions_revoke_all",
            Self::AuthRefreshReuseDetected => "auth.refresh_reuse_detected",
            Self::SignupDisposableBlocked => "signup.disposable_blocked",
            Self::SignupRateLimited => "signup.rate_limited",
            Self::InstanceSetupCompleted => "instance.setup_completed",
        }
    }
}
