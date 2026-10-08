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

/// The accent colour of a team or a workspace: one closed palette shared by
/// both.
///
/// A closed set, not a free string or a hex code. The frontend resolves it
/// through `bgClasses[color]` in `WsIcon.tsx`, keyed by these exact names, so
/// anything outside the list would render with no background at all. The
/// palette lives in `frontend/src/lib/ws-color.ts` (`WS_COLORS`) and in the
/// `--color-ws-*` variables of `globals.css`; this enum and those two must be
/// changed together.
///
/// Serde refuses any other value on its own, so no separate validation rule
/// is needed: an unknown colour fails deserialization and comes back as a 422
/// through `JsonBody`.
///
/// Read back from the database with `parse().unwrap_or_default()`, falling
/// back to `Ember` (see `Default`): a display field must not fail a read.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum AccentColor {
    Ember,
    Blue,
    Violet,
    Teal,
    Rose,
    Amber,
}

impl AccentColor {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccentColor::Ember => "ember",
            AccentColor::Blue => "blue",
            AccentColor::Violet => "violet",
            AccentColor::Teal => "teal",
            AccentColor::Rose => "rose",
            AccentColor::Amber => "amber",
        }
    }
}

impl std::str::FromStr for AccentColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ember" => Ok(AccentColor::Ember),
            "blue" => Ok(AccentColor::Blue),
            "violet" => Ok(AccentColor::Violet),
            "teal" => Ok(AccentColor::Teal),
            "rose" => Ok(AccentColor::Rose),
            "amber" => Ok(AccentColor::Amber),
            _ => Err("Invalid accent color".into()),
        }
    }
}

/// The fallback for a stored value outside the palette, written by hand or
/// before a column was constrained. The first palette entry, and the colour
/// a personal team is created with.
impl Default for AccentColor {
    fn default() -> Self {
        AccentColor::Ember
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Positive: every palette entry survives the round trip through the form
    /// it is stored in, so a row written today reads back as the same colour.
    #[test]
    fn accent_color_round_trips_through_its_stored_form() {
        let palette = [
            AccentColor::Ember,
            AccentColor::Blue,
            AccentColor::Violet,
            AccentColor::Teal,
            AccentColor::Rose,
            AccentColor::Amber,
        ];

        for color in palette {
            let stored = color.as_str();
            assert_eq!(
                stored.parse::<AccentColor>(),
                Ok(color),
                "{stored} must parse back to the variant it came from"
            );
        }
    }

    /// Negative: anything outside the palette is refused at the edge. This is
    /// what replaces a validation rule: a hex code is the shape the frontend
    /// cannot resolve, so it must not get as far as the database.
    #[test]
    fn accent_color_rejects_a_value_outside_the_palette() {
        for rejected in ["#EB5927", "ember ", "Ember", "orange", ""] {
            assert!(
                rejected.parse::<AccentColor>().is_err(),
                "{rejected:?} must not be accepted as an accent colour"
            );
        }
    }

    /// A row holding something outside the palette still renders, falling back
    /// rather than failing the whole read.
    #[test]
    fn stored_color_outside_the_palette_falls_back() {
        assert_eq!(
            "#000000".parse::<AccentColor>().unwrap_or_default(),
            AccentColor::Ember
        );
    }
}
