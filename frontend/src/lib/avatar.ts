// src/lib/avatar.ts
import { API_BASE_URL } from "@/lib/api";

/** Whose avatar: the path segment `GET /avatars/{kind}/...` takes. */
export type AvatarKind = "users" | "teams" | "workspaces";

/** The two renditions the backend stores, in pixels. */
export type AvatarSize = 256 | 64;

/**
 * The URL of an avatar, built as the backend documents it
 * (`/api/v1/avatars/{kind}/{id}/{avatar_version}/{size}.webp`), or `null`
 * when there is no version to show: no avatar, or one the API withheld
 * because the reader could not load it. A `null` is the cue for the
 * monogram, never a URL bound to answer 404.
 */
export function avatarUrl(
  kind: AvatarKind,
  id: string,
  version: string | null | undefined,
  size: AvatarSize,
): string | null {
  if (!version) return null;
  const path = [kind, id, version].map(encodeURIComponent).join("/");
  return `${API_BASE_URL}/avatars/${path}/${size}.webp`;
}
