import { useTranslation } from "react-i18next";

import { useLegalDocument, type LegalSlug } from "@/api/legal";
import { LegalDocument } from "@/components/legal/LegalDocument";
import { usePageTitle } from "@/hooks/usePageTitle";

interface LegalPageProps {
  slug: LegalSlug;
}

const DATE_FORMAT: Intl.DateTimeFormatOptions = {
  year: "numeric",
  month: "long",
  day: "numeric",
};

/**
 * Formats the frontmatter `updated` field. Returns null rather than
 * "Invalid Date" when the value is missing or not a parseable date, which
 * happens if a document still carries the unresolved {{LAST_UPDATED}} token.
 */
function formatUpdatedDate(raw: string, locale: string): string | null {
  if (!raw) return null;
  const parsed = new Date(raw);
  if (Number.isNaN(parsed.getTime())) return null;
  return parsed.toLocaleDateString(locale, DATE_FORMAT);
}

// Rendered inside DocumentShell's content column, which already owns the
// page-level layout (max width, horizontal/vertical padding, centering).
// This component only lays out its own content, it must not reintroduce
// mx-auto/max-w/px/py of its own or it drifts out of alignment with the
// BackButton column next to it.
export function LegalPage({ slug }: Readonly<LegalPageProps>) {
  const { t, i18n } = useTranslation("common");
  const { data, isPending, isError } = useLegalDocument(slug);
  usePageTitle(data?.meta.title ?? t(`legal.${slug}.title`));

  if (isPending) {
    return (
      <div className="space-y-4">
        <div className="h-8 w-2/3 animate-pulse rounded bg-muted" />
        <div className="h-4 w-1/3 animate-pulse rounded bg-muted" />
        <div className="h-64 animate-pulse rounded bg-muted" />
      </div>
    );
  }

  if (isError || !data) {
    return <p className="text-muted-foreground">{t("legal.loadError")}</p>;
  }

  const formattedDate = formatUpdatedDate(data.meta.updated, i18n.language);

  return (
    <article>
      {(formattedDate || data.meta.version) && (
        <p className="mb-8 text-sm text-muted-foreground">
          {formattedDate && (
            <>
              {t("legal.lastUpdated")}{" "}
              <time dateTime={data.meta.updated}>{formattedDate}</time>
            </>
          )}
          {formattedDate && data.meta.version && " \u00b7 "}
          {data.meta.version && `${t("legal.version")} ${data.meta.version}`}
        </p>
      )}

      {/* Falls back to the raw token only if the document is misconfigured. */}
      <LegalDocument doc={data} formattedDate={formattedDate ?? data.meta.updated} />
    </article>
  );
}
