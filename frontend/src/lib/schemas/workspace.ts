import { z } from "zod";

import type {
  AccentColor,
  Platform,
  UpdateWorkspaceInput,
  WorkspaceMode,
  WorkspaceType,
  WriteupVisibility,
} from "@/lib/api/types";

// The values of the generated enums, for zod and for the pickers. `satisfies` makes tsc
// refuse any value the API does not know; a value the API adds still has to be added here.
export const ACCENT_COLORS = ["ember", "blue", "violet", "teal", "rose", "amber"] as const satisfies readonly AccentColor[];
export const PLATFORMS = ["htb", "thm", "root_me", "ctftime", "other"] as const satisfies readonly Platform[];
export const WORKSPACE_TYPES = ["platform", "event"] as const satisfies readonly WorkspaceType[];
export const WORKSPACE_MODES = ["solo", "team"] as const satisfies readonly WorkspaceMode[];
export const WRITEUP_VISIBILITIES = ["draft", "team"] as const satisfies readonly WriteupVisibility[];

/** The value of an <input type="datetime-local">: local time, no zone. Empty when cleared. */
const localDateTime = z.string().nullable();

/** The name the API checks again: blank is refused here, length and sluggability there. */
const name = z.string().trim().min(1, "workspaces:validation.nameRequired");

function endsAfterStart(v: { starts_at: string | null; ends_at: string | null }, ctx: z.RefinementCtx) {
  // datetime-local values share one format, so comparing them as strings orders them.
  if (v.starts_at && v.ends_at && v.ends_at <= v.starts_at) {
    ctx.addIssue({ code: "custom", path: ["ends_at"], message: "workspaces:validation.endsBeforeStart" });
  }
}

// ---- Creation ---------------------------------------------------------------

export const createWorkspaceSchema = z
  .object({
    name,
    description: z.string().optional(),
    color: z.enum(ACCENT_COLORS),
    type: z.enum(WORKSPACE_TYPES),
    platform: z.enum(PLATFORMS).nullable(),
    mode: z.enum(WORKSPACE_MODES),
    team_id: z.uuid().nullable(),
    starts_at: localDateTime,
    ends_at: localDateTime,
    lock_after_end: z.boolean(),
  })
  .superRefine((v, ctx) => {
    if (v.type === "platform" && !v.platform) {
      ctx.addIssue({ code: "custom", path: ["platform"], message: "workspaces:validation.platformRequired" });
    }
    if (v.mode === "team" && !v.team_id) {
      ctx.addIssue({ code: "custom", path: ["team_id"], message: "workspaces:validation.teamRequired" });
    }
    endsAfterStart(v, ctx);
  });

export type CreateWorkspaceValues = z.infer<typeof createWorkspaceSchema>;

// ---- Settings ----------------------------------------------------------

const EVENT_URL_MAX = 2048;

/** Mirrors the API: an absolute http(s) URL with a host and no credentials. Empty clears it. */
function isEventUrl(value: string): boolean {
  if (value === "") return true;
  if (value.length > EVENT_URL_MAX) return false;
  try {
    const url = new URL(value);
    return (url.protocol === "http:" || url.protocol === "https:") && url.hostname !== "" && !url.username && !url.password;
  } catch {
    return false;
  }
}

export const updateWorkspaceSchema = z
  .object({
    name,
    description: z.string(),
    color: z.enum(ACCENT_COLORS),
    platform: z.enum(PLATFORMS).nullable(),
    event_url: z.string().trim().refine(isEventUrl, { message: "workspaces:validation.eventUrl" }),
    starts_at: localDateTime,
    ends_at: localDateTime,
    lock_after_end: z.boolean(),
    default_writeup_visibility: z.enum(WRITEUP_VISIBILITIES),
  })
  .superRefine(endsAfterStart);

export type UpdateWorkspaceValues = z.infer<typeof updateWorkspaceSchema>;

/** A datetime-local value as the UTC instant the API takes; empty means none. */
export function toInstant(local: string | null): string | null {
  return local ? new Date(local).toISOString() : null;
}

/** An API instant as a datetime-local value, in the browser's zone. */
export function toLocalInput(instant: string | null | undefined): string | null {
  if (!instant) return null;
  const date = new Date(instant);
  const shifted = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return shifted.toISOString().slice(0, 16);
}

/** Only what changed. An emptied description or event URL is sent as null, which clears it. */
export function toWorkspacePatch(
  values: UpdateWorkspaceValues,
  dirty: Partial<Record<keyof UpdateWorkspaceValues, boolean | undefined>>,
): UpdateWorkspaceInput {
  const patch: UpdateWorkspaceInput = {};
  if (dirty.name) patch.name = values.name.trim();
  if (dirty.description) patch.description = values.description.trim() || null;
  if (dirty.color) patch.color = values.color;
  if (dirty.platform) patch.platform = values.platform;
  if (dirty.event_url) patch.event_url = values.event_url.trim() || null;
  if (dirty.starts_at) patch.starts_at = toInstant(values.starts_at);
  if (dirty.ends_at) patch.ends_at = toInstant(values.ends_at);
  if (dirty.lock_after_end) patch.lock_after_end = values.lock_after_end;
  if (dirty.default_writeup_visibility) patch.default_writeup_visibility = values.default_writeup_visibility;
  return patch;
}
