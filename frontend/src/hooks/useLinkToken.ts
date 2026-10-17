// src/hooks/useLinkToken.ts
import { useEffect, useState } from "react";

/**
 * Reads the token a mailed link carries in its fragment (`#token=`), the part
 * of a URL the browser never sends, so it stays out of every access log. The
 * query string is never read: a token there would already have reached the
 * server's logs.
 */
function readLinkToken(location: Location): string | null {
  return new URLSearchParams(location.hash.replace(/^#/, "")).get("token");
}

/**
 * The token of the link that opened this page, read once, then dropped from
 * the address bar and from the history entry: kept in memory only, it no
 * longer sits where the history, a screenshot or an extension would find it.
 */
export function useLinkToken(): string | null {
  const [token] = useState(() => readLinkToken(window.location));

  useEffect(() => {
    if (token === null) return;
    const url = new URL(window.location.href);
    url.hash = "";
    // The router's own state rides along in `history.state`; keep it.
    window.history.replaceState(window.history.state, "", `${url.pathname}${url.search}`);
  }, [token]);

  return token;
}
