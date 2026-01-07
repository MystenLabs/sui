import React from "react";
import CodeBlock from "@theme/CodeBlock";
import utils from "./utils";
import MarkdownIt from "markdown-it";
import { importContentMap } from "../../.generated/ImportContentMap";

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

function resolveMdxComponent(mod: AnyMod): ResolvedComp {
  const cand = mod?.default ?? mod;
  const maybe = cand?.default ?? cand;
  return typeof maybe === "function"
    ? (maybe as React.ComponentType<any>)
    : null;
}

const SNIPPET_MAP: Record<string, React.ComponentType<any>> = {};
snippetReq.keys().forEach((k: string) => {
  const Comp = resolveMdxComponent(snippetReq<AnyMod>(k));
  if (!Comp) return;
  const key = k.replace(/^\.\//, ""); // "sub/x.mdx"
  SNIPPET_MAP[key] = Comp;
  SNIPPET_MAP[key.replace(/\.mdx?$/, "")] = Comp; // also without extension
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

  if (mode === "snippet") {
    const normalized = source.replace(/^\.\//, "");
    const Comp =
      SNIPPET_MAP[normalized] ||
      SNIPPET_MAP[normalized.replace(/\.mdx?$/, "")] ||
      SNIPPET_MAP[`${normalized}.mdx`] ||
      SNIPPET_MAP[`${normalized}.md`];

    if (!Comp) {
      return (
        <div className="alert alert--warning" role="alert">
          Missing or invalid snippet: <code>{source}</code>
        </div>
      );
    }
    return <Comp />;
  }

  // mode === "code"
  // Expect paths like "packages/…", "apps/…", "docs/…"
  const cleaned = source.replace(/^\/+/, "").replace(/^\.\//, "");

  const match = cleaned.match(/\.([^.]+)$/);
  const ext = match ? match[1] : undefined;

  // If language is not explicitly set, use extension
  if (!language) {
    switch (ext) {
      case "lock":
        language = "toml";
        break;
      case "sh":
        language = "shell";
        break;
      case "mdx":
        language = "markdown";
        break;
      case "tsx":
        language = "ts";
        break;
      case "rs":
        language = "rust";
        break;
      case "move":
        language = "move";
        break;
      case "prisma":
        language = "ts";
        break;
      default:
        language = ext || "text";
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
    content = ghText;
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
    out = utils.returnFunctions(out, fun, language, signatureOnly);
  }

  if (variable) {
    out = utils.returnVariables(out, variable, language);
  }

  if (struct) {
    out = utils.returnStructs(out, struct, language);
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
  if (/^m(?:d|arkdown)$/i.test(style)) {
    const html = md.render(out);
    return (
      <div
        className="import-content--nofence mdx-content"
        dangerouslySetInnerHTML={{ __html: html }}
      />
    );
  }

  return (
    <CodeBlock language={language} metastring={meta}>
      {out}
    </CodeBlock>
  );
}
