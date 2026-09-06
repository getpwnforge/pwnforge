import { useEffect } from "react";
import { Link, useLocation } from "react-router";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeSlug from "rehype-slug";

import type { LegalDoc } from "@/api/legal";

/**
 * Additional dependency required:
 *   pnpm add rehype-slug
 *
 * rehype-slug generates the heading ids that the in-document anchors
 * (#7-politique-de-marque..., #9-durees-de-conservation, ...) point to.
 * Without it every cross-reference in the legal documents is a dead link.
 */

const REMARK_PLUGINS = [remarkGfm];
const REHYPE_PLUGINS = [rehypeSlug];

const LINK_CLASS = "text-primary underline underline-offset-4 hover:text-primary/80";

const MD_COMPONENTS: Components = {
  a: ({ href, children }) => {
    if (!href) return <>{children}</>;

    // In-page anchor.
    if (href.startsWith("#")) {
      return (
        <a href={href} className={LINK_CLASS}>
          {children}
        </a>
      );
    }

    // Internal route, possibly carrying a hash (e.g. /privacy#9-durees-de-conservation).
    if (href.startsWith("/")) {
      return (
        <Link to={href} className={LINK_CLASS}>
          {children}
        </Link>
      );
    }

    return (
      <a href={href} target="_blank" rel="noopener noreferrer" className={LINK_CLASS}>
        {children}
      </a>
    );
  },

  // The processor and retention tables are wide; let them scroll rather than
  // overflow the viewport on mobile.
  table: ({ children }) => (
    <div className="my-6 overflow-x-auto rounded-md border border-border">
      <table className="w-full border-collapse text-sm">{children}</table>
    </div>
  ),
  th: ({ children }) => (
    <th className="border-b border-border bg-muted/40 px-3 py-2 text-left font-medium">
      {children}
    </th>
  ),
  td: ({ children }) => (
    <td className="border-b border-border px-3 py-2 align-top">{children}</td>
  ),

  h1: ({ children }) => (
    <h1 className="mb-2 text-3xl font-semibold tracking-tight">{children}</h1>
  ),
  h2: ({ children, id }) => (
    <h2
      id={id}
      className="mt-12 scroll-mt-24 border-b border-border pb-2 text-xl font-semibold tracking-tight"
    >
      {children}
    </h2>
  ),
  h3: ({ children, id }) => (
    <h3 id={id} className="mt-8 scroll-mt-24 text-base font-semibold">
      {children}
    </h3>
  ),

  p: ({ children }) => <p className="my-4 leading-7">{children}</p>,
  ul: ({ children }) => <ul className="my-4 list-disc space-y-1 pl-6">{children}</ul>,
  ol: ({ children }) => <ol className="my-4 list-decimal space-y-1 pl-6">{children}</ol>,
  blockquote: ({ children }) => (
    <blockquote className="my-6 border-l-2 border-primary/60 bg-muted/30 px-4 py-2 text-muted-foreground">
      {children}
    </blockquote>
  ),
  code: ({ children }) => (
    <code className="rounded bg-muted px-1.5 py-0.5 font-mono text-[0.85em]">
      {children}
    </code>
  ),
  hr: () => <hr className="my-10 border-border" />,
};

interface LegalDocumentProps {
  doc: LegalDoc;
  /** Formatted date injected in place of the {{LAST_UPDATED}} token. */
  formattedDate: string;
}

export function LegalDocument({ doc, formattedDate }: Readonly<LegalDocumentProps>) {
  const { hash } = useLocation();
  const body = doc.body.replaceAll("{{LAST_UPDATED}}", formattedDate);

  // react-markdown renders after mount, so the browser's native hash scroll
  // has already run against a document without the target element.
  useEffect(() => {
    if (!hash) return;
    const target = document.getElementById(decodeURIComponent(hash.slice(1)));
    target?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [hash, body]);

  return (
    <ReactMarkdown
      remarkPlugins={REMARK_PLUGINS}
      rehypePlugins={REHYPE_PLUGINS}
      components={MD_COMPONENTS}
    >
      {body}
    </ReactMarkdown>
  );
}
