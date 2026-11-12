// src/hooks/useLinkToken.ts
import { useEffect, useState } from "react";

const STASH_PREFIX = "pwnforge:link-token:";

/**
 * Reads the token a mailed link carries in its fragment (`#token=`), the part
 * of a URL the browser never sends, so it stays out of every access log. The
 * query string is never read: a token there would already have reached the
 * server's logs.
 */
function readLinkToken(location: Location): string | null {
  return new URLSearchParams(location.hash.replace(/^#/, "")).get("token");
}

// Storage can be missing or throw (private mode, blocked site data): the link
// then works as before, only not across a sign-in.
function readStash(stash: string): string | null {
  try {
    return window.sessionStorage.getItem(STASH_PREFIX + stash);
  } catch {
    return null;
  }
}

function writeStash(stash: string, token: string): void {
  try {
    window.sessionStorage.setItem(STASH_PREFIX + stash, token);
  } catch {
    // Kept in memory only.
  }
}

/** Drops a stashed token once it is used up, refused, or given up on. */
export function forgetLinkToken(stash: string): void {
  try {
    window.sessionStorage.removeItem(STASH_PREFIX + stash);
  } catch {
    // Nothing was kept.
  }
}

/**
 * The token of the link that opened this page, read once, then dropped from
 * the address bar and from the history entry: it no longer sits where the
 * history, a screenshot or an extension would find it.
 *
 * With `stash`, it is also kept in this tab's sessionStorage, so the page
 * still has it when it comes back from signing in without the fragment. Never
 * in a URL, gone with the tab, and dropped with `forgetLinkToken` as soon as
 * it has served. Only for links worth carrying across a sign-in (invitations),
 * not for a password reset.
 */
export function useLinkToken(stash?: string): string | null {
  const [token] = useState(() => {
    const fromLink = readLinkToken(window.location);
    if (!stash) return fromLink;
    if (fromLink) {
      writeStash(stash, fromLink);
      return fromLink;
    }
    return readStash(stash);
  });

  useEffect(() => {
    if (token === null) return;
    const url = new URL(window.location.href);
    if (!url.hash) return;
    url.hash = "";
    // The router's own state rides along in `history.state`; keep it.
    window.history.replaceState(window.history.state, "", `${url.pathname}${url.search}`);
  }, [token]);

  return token;
}
