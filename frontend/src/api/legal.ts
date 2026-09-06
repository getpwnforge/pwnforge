import { api } from "@/lib/api";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";

/**
 * Legal documents are served as static files from `frontend/public/legal/`
 * rather than bundled from `src/`, so that a self-hosted deployment can
 * override them with a volume mount without rebuilding the image:
 *
 *   volumes:
 *     - ./legal:/usr/share/nginx/html/legal:ro
 */

export const LEGAL_SLUGS = ["legal", "privacy", "terms"] as const;
export type LegalSlug = (typeof LEGAL_SLUGS)[number];

const SUPPORTED_LANGS = ["fr", "en"] as const;
const FALLBACK_LANG = "fr";

export interface LegalMeta {
  slug: LegalSlug;
  lang: string;
  title: string;
  description: string;
  version: string;
  updated: string;
}

export interface LegalDoc {
  meta: LegalMeta;
  body: string;
}

export interface LegalVersions {
  terms_version: string;
  privacy_version: string;
}

export const LEGAL_VERSIONS_QUERY_KEY = ["legal", "versions"] as const;

export type LegalState = "ok" | "pending";

export interface LegalStatus {
  state: LegalState;
  current_terms_version: string;
  current_privacy_version: string;
  accepted_terms_version: string | null;
  accepted_privacy_version: string | null;
  effective_at: string | null;
}

export const LEGAL_STATUS_QUERY_KEY = ["legal", "status"] as const;

const FRONTMATTER_RE = /^---\r?\n([\s\S]*?)\r?\n---\r?\n/;

/**
 * Minimal frontmatter parser. The legal documents only use flat `key: value`
 * pairs, so a dedicated library (and its Buffer polyfill) is not warranted.
 */
function parseFrontmatter(raw: string): { meta: Record<string, string>; body: string } {
  const match = FRONTMATTER_RE.exec(raw);
  if (!match) return { meta: {}, body: raw };

  const meta: Record<string, string> = {};
  for (const line of match[1].split(/\r?\n/)) {
    const separator = line.indexOf(":");
    if (separator === -1) continue;
    const key = line.slice(0, separator).trim();
    const value = line.slice(separator + 1).trim().replace(/^["']|["']$/g, "");
    if (key) meta[key] = value;
  }

  return { meta, body: raw.slice(match[0].length) };
}

function resolveLang(language: string): string {
  const base = language.split("-")[0];
  return (SUPPORTED_LANGS as readonly string[]).includes(base) ? base : FALLBACK_LANG;
}

async function fetchLegalDoc(slug: LegalSlug, lang: string): Promise<LegalDoc> {
  const response = await fetch(`/legal/${lang}/${slug}.md`, {
    headers: { Accept: "text/markdown, text/plain" },
  });

  if (!response.ok) {
    throw new Error(`legal document not found: ${lang}/${slug}`);
  }

  const raw = await response.text();

  // nginx falls back to index.html for unknown paths, so a 200 does not
  // guarantee we received markdown. Reject anything without frontmatter.
  if (!FRONTMATTER_RE.test(raw)) {
    throw new Error(`legal document malformed: ${lang}/${slug}`);
  }

  const { meta, body } = parseFrontmatter(raw);

  return {
    meta: {
      slug,
      lang: meta.lang ?? lang,
      title: meta.title ?? "",
      description: meta.description ?? "",
      version: meta.version ?? "",
      updated: meta.updated ?? "",
    },
    body,
  };
}

export function useLegalDocument(slug: LegalSlug) {
  const { i18n } = useTranslation();
  const lang = resolveLang(i18n.language);

  return useQuery({
    queryKey: ["legal", slug, lang],
    queryFn: async () => {
      try {
        return await fetchLegalDoc(slug, lang);
      } catch (error) {
        if (lang === FALLBACK_LANG) throw error;
        return fetchLegalDoc(slug, FALLBACK_LANG);
      }
    },
    staleTime: Infinity,
    gcTime: Infinity,
    retry: 1,
  });
}

async function fetchLegalVersions(): Promise<LegalVersions> {
  const { data } = await api.get<LegalVersions>("/legal/versions");
  return data;
}

export async function acceptLegal(versions: LegalVersions): Promise<void> {
  await api.post("/legal/accept", versions);
}

export function useLegalVersions() {
  return useQuery({
    queryKey: LEGAL_VERSIONS_QUERY_KEY,
    queryFn: fetchLegalVersions,
    // Rarely changes, but not immutable: a revision published while the SPA
    // is open must eventually be picked up.
    staleTime: 5 * 60 * 1000,
  });
}

export function useLegalStatus(enabled: boolean) {
  return useQuery({
    queryKey: LEGAL_STATUS_QUERY_KEY,
    queryFn: async () => (await api.get<LegalStatus>("/legal/status")).data,
    // Authenticated route, unlike /alerts/active: querying it while signed
    // out returns 401 and would surface as an error state.
    enabled,
    staleTime: 5 * 60 * 1000,
    retry: false,
  });
}
