import { Renderer, marked, type Tokens } from "marked";
import DOMPurify from "isomorphic-dompurify";

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

const renderer = new Renderer();

renderer.html = function html({ text }: Tokens.HTML | Tokens.Tag): string {
  return escapeHtml(text);
};

const purifyConfig = {
  ALLOW_DATA_ATTR: false,
  ALLOWED_URI_REGEXP:
    /^(?:(?:https?|mailto|tel):|[^a-z]|[a-z+.-]+(?:[^a-z+.-:]|$))/i,
};

export function renderMarkdown(markdown: string): string {
  try {
    const html = marked.parse(markdown, {
      async: false,
      gfm: true,
      breaks: false,
      renderer,
    });

    return DOMPurify.sanitize(String(html), purifyConfig);
  } catch (error) {
    console.error("[markdown] render failed", error);
    return `<p>${escapeHtml(markdown)}</p>`;
  }
}
