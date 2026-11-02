import { z } from "zod";

import { ACCENT_COLORS } from "@/lib/schemas/workspace";

/** Blank is refused here; length, reserved and unsluggable names are the API's (422 under the field). */
export const teamSchema = z.object({
  name: z.string().trim().min(1, "teams:validation.nameRequired"),
  color: z.enum(ACCENT_COLORS),
});

export type TeamValues = z.infer<typeof teamSchema>;

export const teamSettingsSchema = teamSchema.extend({
  description: z.string().max(500, "teams:validation.descriptionTooLong"),
  members_can_create_workspaces: z.boolean(),
});
export type TeamSettingsValues = z.infer<typeof teamSettingsSchema>;
