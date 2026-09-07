// src/lib/timezones.ts

export type TimeZoneOption = {
  /** IANA identifier — the only thing ever sent to the backend. */
  id: string;
  /** `Europe/Paris (GMT+02:00)`, built for display only. */
  label: string;
};

/** The zone the browser is in, falling back to UTC on a runtime without one. */
export function resolvedTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
}

function currentOffset(timeZone: string, at: Date): string {
  try {
    const parts = new Intl.DateTimeFormat("en-US", {
      timeZone,
      timeZoneName: "longOffset",
    }).formatToParts(at);

    return parts.find((part) => part.type === "timeZoneName")?.value ?? "";
  } catch {
    // An identifier the runtime does not know: keep it selectable, unlabelled.
    return "";
  }
}

/**
 * Every zone the runtime knows, labelled with the offset **in effect today**.
 *
 * The offset is decoration: it moves twice a year, which is exactly why the
 * value submitted is the identifier and never the offset itself.
 */
export function listTimeZones(): TimeZoneOption[] {
  const at = new Date();

  const ids =
    typeof Intl.supportedValuesOf === "function"
      ? Intl.supportedValuesOf("timeZone")
      : // Runtime too old to enumerate them: the operator can still keep
        // whatever the browser reports.
        [resolvedTimeZone(), "UTC"];

  const unique = Array.from(new Set(ids));

  return unique.map((id) => {
    const offset = currentOffset(id, at);
    return { id, label: offset ? `${id} (${offset})` : id };
  });
}
