
import React from "react";
import CodeBlock from "@theme/CodeBlock";
import { MDXProvider } from "@mdx-js/react";
import MDXComponents from "@theme/MDXComponents";
import utils from "./utils";
import MarkdownIt from "markdown-it";

import { importContentMap } from "../../../.generated/ImportContentMap";

/// <reference types="webpack-env" />

/** ------------------ SNIPPET MODE (scoped to /snippets) ------------------ */
// eslint-disable-next-line @typescript-eslint/no-var-requires
const snippetReq: __WebpackModuleApi.RequireContext = require.context(
  "@docs/snippets",
  true,
  /\.mdx?$/,
);

type AnyMod = any;
type ResolvedComp = React.ComponentType<any> | null;

/**
 * Resolves an MDX module to a React component.
 * Handles various export patterns from MDX v3 / Docusaurus 3.x.
 */
function resolveMdxComponent(mod: AnyMod): ResolvedComp {
  if (!mod) return null;

  // MDX v3 / Docusaurus 3.x typically exports as mod.default
  const candidate = mod.default ?? mod;

  // Sometimes there's a double-default wrapper
  const component = candidate?.default ?? candidate;

  // Verify it's actually a function (React component)
  if (typeof component === "function") {
    return component as React.ComponentType<any>;
  }

  // Check if it's a valid React element type (could be a forwardRef or memo)
  if (
    component &&
    typeof component === "object" &&
    (component.$$typeof === Symbol.for("react.forward_ref") ||
      component.$$typeof === Symbol.for("react.memo"))
  ) {
    return component as React.ComponentType<any>;
  }

  return null;
}

/**
 * Validates that a value is a renderable React component.
 */
function isValidComponent(comp: unknown): comp is React.ComponentType<any> {
  if (!comp) return false;

  // Function components
  if (typeof comp === "function") return true;

  // forwardRef, memo, lazy components
  if (
    typeof comp === "object" &&
    comp !== null &&
    "$$typeof" in comp &&
    typeof (comp as any).$$typeof === "symbol"
  ) {
    return true;
  }

  return false;
}

const SNIPPET_MAP: Record<string, React.ComponentType<any>> = {};

// Build the snippet map at module load time
snippetReq.keys().forEach((k: string) => {
  try {
    const raw = snippetReq<AnyMod>(k);
    const Comp = resolveMdxComponent(raw);

    if (!isValidComponent(Comp)) {
      if (process.env.NODE_ENV === "development") {
        console.warn(`[ImportContent] Skipping invalid snippet: ${k}`);
      }
      return;
    }

    const key = k.replace(/^\.\//, ""); // "sub/x.mdx"
    SNIPPET_MAP[key] = Comp;
    SNIPPET_MAP[key.replace(/\.mdx?$/, "")] = Comp; // also without extension
  } catch (err) {
    if (process.env.NODE_ENV === "development") {
      console.error(`[ImportContent] Error loading snippet ${k}:`, err);
    }
  }
});

type Props = {
  /** For mode="snippet": path under /snippets. For mode="code": repo-relative path like "packages/foo/src/x.ts". */
  source: string;
  mode: "snippet" | "code";
  language?: string; // for CodeBlock (code mode)
  tag?: string; // ID using the docs:: comment format
  fun?: string; // target functions
  variable?: string;
  struct?: string;
  impl?: string;
  type?: string;
  trait?: string;
  enumeration?: string;
  module?: string;
  component?: string;
  dep?: string;
  test?: string; // target test blocks
  highlight?: string;
  noComments?: boolean; // if included, remove ALL code comments
  noTests?: boolean; // if included, don't include tests
  noTitle?: boolean;
  style?: string;
  org?: string;
  repo?: string;
  ref?: string;
  signatureOnly?: boolean; // if included, only display function signature
};

export default function ImportContent({
  source,
  mode,
  language,
  tag,
  noComments,
  noTests,
  noTitle,
  fun,
  variable,
  struct,
  type,
  impl,
  trait,
  enumeration, // enum is reserved word
  module,
  dep,
  component,
  test,
  highlight,
  style,
  org,
  repo,
  ref,
  signatureOnly,
}: Props) {
  const md = React.useMemo(
    () => new MarkdownIt({ html: true, linkify: true, typographer: true }),
    [],
  );
  const [ghText, setGhText] = React.useState<string | null>(null);
  const [ghErr, setGhErr] = React.useState<string | null>(null);
  const [ghLoading, setGhLoading] = React.useState(false);

  const isGitHub = Boolean(org && repo);

  React.useEffect(() => {
    let cancelled = false;
    async function run() {
      if (!isGitHub) return;
      setGhLoading(true);
      setGhErr(null);
      try {
        const branch = ref || "main";
        const path = String(source || "").replace(/^\.\/?/, "");
        const url = `https://raw.githubusercontent.com/${org}/${repo}/${branch}/${path}`;
        const headers: Record<string, string> = {};

        const res = await fetch(url, { headers });
        if (!res.ok) throw new Error(`GitHub fetch failed: ${res.status}`);
        const text = await res.text();
        if (!cancelled) setGhText(text);
      } catch (e: any) {
        if (!cancelled) setGhErr(e?.message || "Failed to fetch from GitHub");
      } finally {
        if (!cancelled) setGhLoading(false);
      }
    }
    run();
    return () => {
      cancelled = true;
    };
  }, [isGitHub, org, repo, ref, source]);

  // Handle snippet mode
  if (mode === "snippet") {
    const normalized = source.replace(/^\.\//, "");
    const Comp =
      SNIPPET_MAP[normalized] ||
      SNIPPET_MAP[normalized.replace(/\.mdx?$/, "")] ||
      SNIPPET_MAP[`${normalized}.mdx`] ||
      SNIPPET_MAP[`${normalized}.md`];

    // Validate component before rendering
    if (!isValidComponent(Comp)) {
      return (
        <div className="alert alert--warning" role="alert">
          Missing or invalid snippet: <code>{source}</code>
          {process.env.NODE_ENV === "development" && (
            <div style={{ fontSize: "0.8em", marginTop: "0.5em" }}>
              <details>
                <summary>Debug info</summary>
                <pre>
                  {JSON.stringify(
                    {
                      normalized,
                      availableKeys: Object.keys(SNIPPET_MAP).slice(0, 20),
                      compType: typeof Comp,
                    },
                    null,
                    2,
                  )}
                </pre>
              </details>
            </div>
          )}
        </div>
      );
    }

    // Wrap with MDXProvider so that components (Tabs, TabItem, etc.)
    // imported inside the snippet MDX files resolve correctly
    return (
      <MDXProvider components={MDXComponents}>
        <Comp />
      </MDXProvider>
    );
  }

  // mode === "code"
  // Expect paths like "packages/…", "apps/…", "docs/…"
  const cleaned = source.replace(/^\/+/, "").replace(/^\.\//, "");

  const match = cleaned.match(/\.([^.]+)$/);
  const ext = match ? match[1] : undefined;

  // If language is not explicitly set, use extension
  let resolvedLanguage = language;
  if (!resolvedLanguage) {
    switch (ext) {
      case "lock":
        resolvedLanguage = "toml";
        break;
      case "sh":
        resolvedLanguage = "shell";
        break;
      case "mdx":
        resolvedLanguage = "markdown";
        break;
      case "tsx":
        resolvedLanguage = "ts";
        break;
      case "rs":
        resolvedLanguage = "rust";
        break;
      case "move":
        resolvedLanguage = "move";
        break;
      case "prisma":
        resolvedLanguage = "ts";
        break;
      default:
        resolvedLanguage = ext || "text";
    }
  }

  if (isGitHub && ghLoading) {
    return <div className="import-content loading">Loading…</div>;
  }

  if (isGitHub && ghErr) {
    return <pre className="import-content error">{ghErr}</pre>;
  }

  let content: string;
  if (isGitHub) {
    content = ghText as string;
  } else {
    content = importContentMap[cleaned];
  }

  if (content == null) {
    return (
      <div className="alert alert--warning" role="alert">
        File not found in manifest: <code>{cleaned}</code>. You probably need to
        run `pnpm prebuild` and restart the site.
      </div>
    );
  }

  let out = content
    .replace(
      /^\/\/\s*Copyright.*Mysten Labs.*\n\/\/\s*SPDX-License.*?\n?$/gim,
      "",
    )
    .replace(
      /\[dependencies\]\nsui\s?=\s?{\s?local\s?=.*sui-framework.*\n/i,
      "[dependencies]",
    );

  if (tag) {
    out = utils.returnTag(out, tag);
  }

  if (module) {
    out = utils.returnModules(out, module);
  }

  if (component) {
    out = utils.returnComponents(source, component);
  }

  if (fun) {
    out = utils.returnFunctions(out, fun, resolvedLanguage, signatureOnly);
  }

  if (variable) {
    out = utils.returnVariables(out, variable, resolvedLanguage);
  }

  if (struct) {
    out = utils.returnStructs(out, struct, resolvedLanguage);
  }

  if (type) {
    out = utils.returnTypes(out, type);
  }

  if (impl) {
    out = utils.returnImplementations(out, impl);
  }

  if (trait) {
    out = utils.returnTraits(out, trait);
  }

  if (enumeration) {
    out = utils.returnEnums(out, enumeration);
  }

  if (dep) {
    out = utils.returnDeps(out, dep);
  }

  if (test) {
    out = utils.returnTests(out, test);
  }

  out = out.replace(/^\s*\/\/\s*docs::\/?.*\r?$\n?/gm, ""); // remove all docs:: style comments

  if (noTests) {
    out = utils.returnNotests(out);
  }

  if (noComments) {
    // get rid of all comments
    out = out.replace(/^ *\/\/.*\n/gm, "");
  }

  // Remove top blank line if exists
  out = out.replace(/^\s*\n/, "");

  // Safely compute highlight metastring
  const title = org ? `github.com/${org}/${repo}/${cleaned}` : cleaned;
  const rawHighlight = typeof highlight === "string" ? highlight : "";
  const computedHL = rawHighlight
    ? utils.highlightLine(out, rawHighlight)
    : null;
  const hl = (computedHL ?? "").toString().trim();
  const isValidHL =
    hl.length > 0 &&
    /^[0-9,\-\s]+$/.test(hl) &&
    hl.toLowerCase() !== "undefined";
  let meta = "";
  if (isValidHL) {
    meta = `{${hl}}`;
    if (!noTitle) meta += ` title="${title}"`;
  } else {
    if (!noTitle) meta = `title="${title}"`;
  }

  // just render markdown if style = "markdown" or "md"
  if (/^m(?:d|arkdown)$/i.test(style || "")) {
    const html = md.render(out);
    return (
      <div
        className="import-content--nofence mdx-content"
        dangerouslySetInnerHTML={{ __html: html }}
      />
    );
  }

  return (
    <CodeBlock language={resolvedLanguage} metastring={meta}>
      {out}
    </CodeBlock>
  );
}
