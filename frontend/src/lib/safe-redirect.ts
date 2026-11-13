// src/lib/safe-redirect.ts
import { ROUTES } from "@/lib/routes";

/**
 * Where to land after signing in.
 *
 * Only same-origin paths are accepted: `//evil.tld` and `https://evil.tld` are
 * both values a browser will happily follow off-site, and this one comes
 * straight from the query string. The router refuses an external target too;
 * this check does not rely on it. A backslash is refused outright, since
 * browsers read `/\evil.tld` as `//evil.tld`, and the rest is resolved the
 * way the browser would resolve it: tabs and newlines dropped, `/./` and
 * `/../` folded. Only a result on this origin is used, and only as resolved.
 *
 * The fragment is dropped, whatever `target` holds: a mailed link carries its
 * token there, and nothing that went through `?from=` may bring one back.
 */
export function safeRedirect(target: string | null): string {
  if (!target || !target.startsWith("/") || target.startsWith("//") || target.includes("\\")) {
    return ROUTES.workspaces;
  }

  let resolved: URL;
  try {
    resolved = new URL(target, window.location.origin);
  } catch {
    return ROUTES.workspaces;
  }

  if (resolved.origin !== window.location.origin) {
    return ROUTES.workspaces;
  }

  return `${resolved.pathname}${resolved.search}`;
}
