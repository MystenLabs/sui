
// src/components/RelatedLink.tsx
import React from "react";
import Link from "@docusaurus/Link";
import { usePluginData } from "@docusaurus/useGlobalData";
import { useLocation } from "@docusaurus/router";

// If you already have these utilities/hooks, great—use them.
// Otherwise, the local fallback resolver below is defensive and won’t crash.
type Meta = {
  id: string;
  path?: string;
  sidebar?: string;
  title?: string;
  description?: string;
  href?: string;
  llmSection?: string;
};

function useDescriptionsSafe(): Meta[] | null {
  try {
    const raw = usePluginData("sui-description-plugin") as any;
    if (!raw) return null;
    if (Array.isArray(raw)) return raw as Meta[];
    if (Array.isArray(raw?.items)) return raw.items as Meta[];
    if (Array.isArray(raw?.descriptions)) return raw.descriptions as Meta[];
    if (Array.isArray(raw?.data)) return raw.data as Meta[];
    if (raw?.byId && typeof raw.byId === "object")
      return Object.values(raw.byId) as Meta[];
    if (typeof raw === "object") {
      const vals = Object.values(raw);
      if (
        vals.length &&
        typeof vals[0] === "object" &&
        "id" in (vals[0] as any)
      ) {
        return vals as Meta[];
      }
    }
    return null;
  } catch {
    return null;
  }
}

function resolveInternal(
  to: string | undefined,
  data: Meta[] | null,
): Meta | undefined {
  if (!to || !data) return undefined;
  // Normalize: remove fragment, strip .md/.mdx, trim trailing slash
  let base = to.split("#")[0].replace(/\.(mdx?|MDX?)$/, "");
  if (base.endsWith("/")) base = base.slice(0, -1);
  const noLead = base.startsWith("/") ? base.slice(1) : base;
  const withLead = base.startsWith("/") ? base : "/" + base;
  return data.find(
    (m) =>
      m.id === noLead ||
      m.id === withLead ||
      m.path === base ||
      m.path === withLead,
  );
}

function humanizeFromPath(p?: string): string | undefined {
  if (!p) return undefined;
  const seg = p.replace(/\/$/, "").split("/").filter(Boolean).pop();
  if (!seg) return undefined;
  return seg
    .split("-")
    .map((s) => s.charAt(0).toUpperCase() + s.slice(1))
    .join(" ");
}

function isExternal(url: string) {
  return /^https?:\/\//i.test(url);
}

function joinAndNormalize(basePath: string, relative: string): string {
  // Ensure base has no trailing slash unless it's root
  const base = basePath === "/" ? "/" : basePath.replace(/\/+$/, "");
  const combined = (base === "/" ? "" : base) + "/" + relative;
  const parts = combined.split("/").filter(Boolean);
  const stack: string[] = [];
  for (const p of parts) {
    if (p === ".") continue;
    if (p === "..") {
      if (stack.length) stack.pop();
      continue;
    }
    stack.push(p);
  }
  return "/" + stack.join("/");
}

function renderInlineMarkdown(input: string): React.ReactNode {
  let key = 0;

  // Helper to process **bold** and *italic* inside a plain (non-code, non-link) string
  const processEmphasis = (text: string): React.ReactNode[] => {
    const outBold: React.ReactNode[] = [];
    const boldParts = text.split(/(\*\*[^*]+\*\*)/g);
    for (const bp of boldParts) {
      if (/^\*\*[^*]+\*\*$/.test(bp)) {
        const inner = bp.slice(2, -2);
        outBold.push(<strong key={key++}>{inner}</strong>);
      } else {
        // Italic inside the remaining parts
        const italicParts = bp.split(/(\*[^*]+\*)/g);
        for (const ip of italicParts) {
          if (/^\*[^*]+\*$/.test(ip)) {
            const inner = ip.slice(1, -1);
            outBold.push(<em key={key++}>{inner}</em>);
          } else if (ip) {
            outBold.push(<React.Fragment key={key++}>{ip}</React.Fragment>);
          }
        }
      }
    }
    return outBold;
  };

  // First, split out links so we don't mutate URLs inside emphasis/code
  const linkRegex = /\[([^\]]+)\]\(([^)\s]+)\)/g; // [text](url)
  const linkOut: React.ReactNode[] = [];
  let cursor = 0;
  let match: RegExpExecArray | null;
  while ((match = linkRegex.exec(input))) {
    const [full, text, url] = match;
    if (match.index > cursor) {
      const before = input.slice(cursor, match.index);
      // Process code/emphasis in the non-link text
      linkOut.push(...processCode(before));
    }
    const isExt = isExternal(url);
    linkOut.push(
      isExt ? (
        <a key={key++} href={url} rel="noopener noreferrer">
          {text}
        </a>
      ) : (
        <Link key={key++} to={url}>
          {text}
        </Link>
      ),
    );
    cursor = match.index + full.length;
  }
  if (cursor < input.length) {
    linkOut.push(...processCode(input.slice(cursor)));
  }

  return <>{linkOut}</>;

  // Split by code spans and process emphasis in between
  function processCode(text: string): React.ReactNode[] {
    const parts = text.split(/(`[^`]+`)/g);
    const out: React.ReactNode[] = [];
    for (const part of parts) {
      if (/^`[^`]+`$/.test(part)) {
        const inner = part.slice(1, -1);
        out.push(<code key={key++}>{inner}</code>);
      } else if (part) {
        out.push(...processEmphasis(part));
      }
    }
    return out;
  }
}

interface Props {
  href?: string; // external link
  to?: string; // internal id (resolved via plugin data)
  label?: string; // optional override for text
  desc?: string; // optional override for description
  className?: string; // allow styling overrides
}

export default function RelatedLink({
  href,
  to,
  label,
  desc,
  className,
}: Props) {
  const data = useDescriptionsSafe();
  const { pathname } = useLocation();
  let normalizedBase: string | undefined = undefined;
  let fragment: string | undefined = undefined;
  if (to) {
    const hashIdx = to.indexOf("#");
    const raw = hashIdx === -1 ? to : to.slice(0, hashIdx);
    if (hashIdx !== -1) fragment = to.slice(hashIdx);
    // Resolve relative paths against current route pathname
    const isAbsolute = raw.startsWith("/");
    const isRelative = !isAbsolute && !isExternal(raw);
    let baseCandidate = isRelative ? joinAndNormalize(pathname, raw) : raw;
    // Strip .md/.mdx and trailing slash
    baseCandidate = baseCandidate.replace(/\.(mdx?|MDX?)$/, "");
    if (baseCandidate.endsWith("/")) baseCandidate = baseCandidate.slice(0, -1);
    normalizedBase = baseCandidate;
  }
  const meta =
    !href && normalizedBase ? resolveInternal(normalizedBase, data) : undefined;

  // Precedence: manual props win ONLY if provided; else fall back to plugin meta; then final fallback
  const text =
    label ??
    meta?.title ??
    humanizeFromPath(meta?.id || normalizedBase) ??
    to ??
    href ??
    "";
  const description = desc ?? meta?.description;

  // Decide destination and component
  let target: string;
  if (href) {
    target = href;
  } else if (meta?.path) {
    target = meta.path + (fragment ?? "");
  } else if (meta?.id && meta.id.startsWith("/")) {
    target = meta.id + (fragment ?? "");
  } else if (meta?.href) {
    target = meta.href + (fragment ?? "");
  } else if (normalizedBase) {
    target = normalizedBase + (fragment ?? "");
  } else {
    target = "#";
  }
  const external = isExternal(target);

  return (
    <div className={`flex items-start gap-2 my-2 ${className ?? ""}`}>
      {/* bullet dot to mimic list item */}
      <span className="mt-1 text-lg leading-none select-none">•</span>

      <div>
        {external ? (
          <a href={target} className="font-medium" rel="noopener noreferrer">
            {text}
          </a>
        ) : (
          <Link to={target} className="font-medium">
            {text}
          </Link>
        )}
        {description && (
          <p className="text-sm opacity-80">
            {renderInlineMarkdown(description)}
          </p>
        )}
      </div>
    </div>
  );
}
