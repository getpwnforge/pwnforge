import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import rehypeHighlight from "rehype-highlight";
import 'highlight.js/styles/github-dark.css';

const MD_COMPONENTS: Components = {
  a: ({ href, children }) => {
    const external = !href?.startsWith("/");
    return (
      <a
        href={href}
        {...(external && { target: "_blank", rel: "noopener noreferrer" })}
        className="text-ember hover:underline"
      >
        {children}
      </a>
    );
  },

  img: ({ src, alt }) => (
    <img src={src} alt={alt ?? ""} loading="lazy" className="max-w-full rounded-md border border-border" />
  ),

  input: ({ checked, type }) =>
    type === "checkbox" ? (
      <input type="checkbox" checked={checked} readOnly className="mr-2 accent-ember" />
    ) : null,

  table: ({ children }) => (
    <div className="overflow-x-auto">
      <table className="w-full border-collapse text-sm">{children}</table>
    </div>
  ),
};


export function MarkdownPreview({ content }: Readonly<{ content: string }>) {
  return (
    <div className="md-preview">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={MD_COMPONENTS}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
