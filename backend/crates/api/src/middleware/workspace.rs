// crates/api/src/middleware/workspace.rs
use std::collections::HashMap;
use std::marker::PhantomData;

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use domain::dto::team::TeamRole;
use domain::dto::workspace::Membership;
use domain::entities::workspaces;
use services::workspace_service::{self, WorkspaceError};
use uuid::Uuid;

use crate::{error::AppError, middleware::auth::AuthUser, state::AppState};

/// Name of the path segment the extractor reads. Every route taking a
/// `WorkspaceMember<_>` must declare it, e.g. `/workspaces/{slug}/members`.
const SLUG_PARAM: &str = "slug";

/// Minimum role a route demands on the workspace named in its path.
///
/// Carried as a type so the requirement is visible in the handler signature,
/// same reasoning as `AuthUser`: it cannot be forgotten when a route is added.
pub trait RequiredRole: Send + Sync + 'static {
    const MIN: TeamRole;
}

/// Zero-sized markers, one per minimum role a route actually demands. Used as
/// `WorkspaceMember<role::Admin>`. No route requires `Member` yet, so there is
/// no marker for it; add one alongside the first route that does.
pub mod role {
    use super::RequiredRole;
    use domain::dto::team::TeamRole;

    pub struct Viewer;
    pub struct Admin;
    pub struct Captain;

    impl RequiredRole for Viewer {
        const MIN: TeamRole = TeamRole::Viewer;
    }
    impl RequiredRole for Admin {
        const MIN: TeamRole = TeamRole::Admin;
    }
    impl RequiredRole for Captain {
        const MIN: TeamRole = TeamRole::Captain;
    }
}

/// An authenticated caller admitted to the workspace in the path with at
/// least `R::MIN`.
///
/// The private `_role` field means only this extractor can build one, so a
/// value of this type is proof the check ran. Handlers taking it must not also
/// take `AuthUser`: `user_id` is here, and the suspension lookup would run twice.
pub struct WorkspaceMember<R> {
    pub user_id: Uuid,
    pub workspace: workspaces::Model,
    pub membership: Membership,
    _role: PhantomData<R>,
}

impl<R: RequiredRole> FromRequestParts<AppState> for WorkspaceMember<R> {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = AuthUser::from_request_parts(parts, state).await?;

        let slug = extract_slug(parts, state).await?;

        let (workspace, membership) =
            workspace_service::get_workspace_for_member(&state.db, &slug, user.id).await?;

        if !membership.satisfies(R::MIN) {
            return Err(AppError::Forbidden);
        }

        Ok(Self {
            user_id: user.id,
            workspace,
            membership,
            _role: PhantomData,
        })
    }
}

/// Reads the `{slug}` segment without assuming it is the route's only
/// parameter, so the extractor keeps working on routes like
/// `/workspaces/{slug}/members/{user_id}`.
///
/// A segment that does not decode names no workspace, so it gets the answer
/// of every other refusal on these routes, body included.
async fn extract_slug(parts: &mut Parts, state: &AppState) -> Result<String, AppError> {
    let map = Path::<HashMap<String, String>>::from_request_parts(parts, state)
        .await
        .map_err(|rejection| {
            tracing::debug!(
                error = ?rejection,
                "failed to extract slug from path parameters"
            );
            WorkspaceError::WorkspaceNotFound
        })?
        .0;

    let slug = map
        .get(SLUG_PARAM)
        .ok_or(WorkspaceError::WorkspaceNotFound)?
        .clone();

    Ok(slug)
}
