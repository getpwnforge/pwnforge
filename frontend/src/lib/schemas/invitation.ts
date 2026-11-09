import { z } from "zod";

import type { CreateInvitationRequest, TeamRole } from "@/lib/api/types";

/** Roles an invitation may give. Captain never: it only moves by transfer. */
export const INVITABLE_ROLES = ["admin", "member", "viewer"] as const satisfies readonly TeamRole[];
export type InvitableRole = (typeof INVITABLE_ROLES)[number];

/** How a person is named in a targeted invitation. */
export const INVITE_TARGETS = ["username", "email"] as const;
export type InviteTarget = (typeof INVITE_TARGETS)[number];

/** Lifetimes offered for a link, in days. 7 is the server's default, 30 its maximum. */
export const LINK_EXPIRY_DAYS = ["1", "7", "14", "30"] as const;

const DAY_MS = 86_400_000;
/** Taken off a chosen lifetime so that 30 days, once on the server, is never past its limit. */
const EXPIRY_MARGIN_MS = 5 * 60_000;

export const inviteToTeamSchema = z
  .object({
    target: z.enum(INVITE_TARGETS),
    who: z.string().trim().min(1, "teams:invitations.validation.whoRequired"),
    role: z.enum(INVITABLE_ROLES),
  })
  .superRefine((values, ctx) => {
    if (values.target === "email" && !z.email().safeParse(values.who).success) {
      ctx.addIssue({ code: "custom", path: ["who"], message: "teams:invitations.validation.emailInvalid" });
    }
  });
export type InviteToTeamValues = z.infer<typeof inviteToTeamSchema>;

export const createTeamLinkSchema = z.object({
  role: z.enum(INVITABLE_ROLES),
  days: z.enum(LINK_EXPIRY_DAYS),
  // Kept as the input's text: empty means no limit.
  max_uses: z
    .string()
    .trim()
    .refine((v) => v === "" || (/^\d+$/.test(v) && Number(v) >= 1), "teams:invitations.validation.maxUses"),
});
export type CreateTeamLinkValues = z.infer<typeof createTeamLinkSchema>;

/** A targeted invitation: by username (a leading @ is dropped) or by address, never both. */
export function toInviteRequest(values: InviteToTeamValues): CreateInvitationRequest {
  const who = values.who.trim();
  return values.target === "email"
    ? { default_role: values.role, invited_email: who }
    : { default_role: values.role, username: who.replace(/^@/, "") };
}

/** A generic link. The default lifetime is left to the server; no limit is sent as nothing. */
export function toLinkRequest(values: CreateTeamLinkValues, now: number = Date.now()): CreateInvitationRequest {
  const body: CreateInvitationRequest = { default_role: values.role };
  if (values.days !== "7") body.expires_at = new Date(now + Number(values.days) * DAY_MS - EXPIRY_MARGIN_MS).toISOString();
  if (values.max_uses !== "") body.max_uses = Number(values.max_uses);
  return body;
}

/** Admin is given by the Captain alone, as for a change of role. */
export function invitableRolesFor(role: TeamRole): readonly InvitableRole[] {
  return role === "captain" ? INVITABLE_ROLES : ["member", "viewer"];
}
