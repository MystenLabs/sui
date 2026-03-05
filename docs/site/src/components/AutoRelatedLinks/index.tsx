/*
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
*/

// src/components/AutoRelatedLinks.tsx
//
// Automatically collects internal links from the current page and injects a
// "Related topics" card grid into the article's DOM so it renders inside the
// content column and aligns with the TOC.
//
// Rules:
//  - Does NOT render if the page contains a <DocCardList /> component
//  - Does NOT render if the page already contains a hand-authored .next-steps-module
//  - Only includes internal links (no external URLs, no static assets)
//  - External links, fragment-only links, and mailto/tel are excluded
//  - Injects into the article element directly so the TOC wraps around it correctly

import React, { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import Link from "@docusaurus/Link";
import { usePluginData } from "@docusaurus/useGlobalData";
import { useLocation } from "@docusaurus/router";

// ── Types ─────────────────────────────────────────────────────────────────────

type Meta = {
  id: string;
  path?: string;
  title?: string;
  description?: string;
  href?: string;
};

type ResolvedLink = {
  href: string;
  title: string;
  description?: string;
  external: boolean;
};

// ── Helpers ───────────────────────────────────────────────────────────────────

function isExternal(url: string) {
  return /^https?:\/\//i.test(url);
}

/** Returns true if the page should be skipped based on DOM content. */
function shouldSkipPage(): boolean {
  // Skip if page has a hand-authored next-steps-module
  if (document.querySelector(".next-steps-module") !== null) return true;
  // Skip if page contains a DocCardList (section index pages)
  if (document.querySelector("[class*='docCardList']") !== null) return true;
  return false;
}

/** Returns true if the link is an internal site path worth including. */
function isInternalContentLink(raw: string): boolean {
  if (!raw) return false;
  // Skip fragment-only, mailto, tel, javascript
  if (/^(#|mailto:|tel:|javascript:)/i.test(raw)) return false;
  const path = raw.split("#")[0];
  // Always include GitHub and move-book URLs
  if (/^https?:\/\/(www\.)?github\.com\//i.test(raw)) return true;
  if (/^https?:\/\/move-book\.com\//i.test(raw)) return true;
  // Skip all other external URLs
  if (isExternal(raw)) return false;
  // Skip empty or root paths
  if (!path || path === "/") return false;
  // Skip static assets
  if (/\.(png|jpe?g|gif|svg|webp|pdf|zip|tar|gz|exe|dmg|pkg)$/i.test(path)) return false;
  return true;
}

function useMetaSafe(): Meta[] | null {
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
      if (vals.length && typeof vals[0] === "object" && "id" in (vals[0] as any))
        return vals as Meta[];
    }
    return null;
  } catch {
    return null;
  }
}

function normalizePath(raw: string): string {
  let p = raw.split("#")[0].replace(/\.(mdx?|MDX?)$/, "");
  if (p.endsWith("/")) p = p.slice(0, -1);
  return p || "/";
}

function resolveMeta(href: string, data: Meta[] | null): Meta | undefined {
  if (!data) return undefined;
  const norm = normalizePath(href);
  const noLead = norm.startsWith("/") ? norm.slice(1) : norm;
  const withLead = norm.startsWith("/") ? norm : "/" + norm;
  return data.find(
    (m) =>
      m.id === noLead ||
      m.id === withLead ||
      m.path === norm ||
      m.path === withLead,
  );
}

/**
 * If the URL is a GitHub link to a specific file (blob or raw), returns
 * { repo, path }. Returns null for repo roots, PRs, issues, and other
 * non-file GitHub URLs.
 */
function parseGithubFilePath(url: string): { repo: string; path: string } | null {
  try {
    const { hostname, pathname } = new URL(url);
    if (!/github\.com$/i.test(hostname) && !/raw\.githubusercontent\.com$/i.test(hostname)) {
      return null;
    }
    const parts = pathname.replace(/^\//, "").split("/");
    if (parts.length < 3) return null;

    const [, repo, type, ...rest] = parts;
    if (!repo || !type) return null;

    const fileLinkTypes = new Set(["blob", "raw"]);
    const isRawHost = /raw\.githubusercontent\.com$/i.test(hostname);

    let fileParts: string[];
    if (isRawHost) {
      fileParts = parts.slice(3);
    } else if (fileLinkTypes.has(type)) {
      fileParts = rest.slice(1); // skip ref
    } else {
      return null;
    }

    if (fileParts.length === 0) return null;
    return { repo, path: fileParts.join("/") };
  } catch {
    return null;
  }
}

// Words that should stay lowercase in title case (unless first/last word)
const LOWERCASE_WORDS = new Set([
  "a", "an", "the", "and", "but", "or", "nor", "for", "so", "yet",
  "at", "by", "in", "of", "on", "to", "up", "as", "is", "it",
  "via", "vs", "with", "from", "into", "onto", "over", "than",
  "that", "upon", "with",
]);

function toTitleCase(str: string): string {
  const words = str.trim().split(/\s+/);
  return words
    .map((word, i) => {
      // Always capitalize first and last word
      if (i === 0 || i === words.length - 1) {
        return word.charAt(0).toUpperCase() + word.slice(1);
      }
      // Preserve all-caps acronyms (e.g. SDK, API, CLI)
      if (word === word.toUpperCase() && word.length > 1) return word;
      // Preserve words with internal caps (e.g. GitHub, MoveVM)
      if (/[A-Z]/.test(word.slice(1))) return word;
      const lower = word.toLowerCase();
      return LOWERCASE_WORDS.has(lower)
        ? lower
        : lower.charAt(0).toUpperCase() + lower.slice(1);
    })
    .join(" ");
}

function humanize(href: string): string {
  const seg = href.replace(/\/$/, "").split("/").filter(Boolean).pop() ?? href;
  const spaced = seg.split(/[-_]/).join(" ");
  return toTitleCase(spaced);
}

// ── Link collector ────────────────────────────────────────────────────────────

/**
 * Extracts the sentence containing the link from its surrounding text.
 * Walks up to the nearest block-level parent, gets its text content,
 * then finds the sentence that contains the link's text.
 */
function extractSurroundingText(a: Element): string | undefined {
  const linkText = a.textContent?.trim() ?? "";

  // Walk up to nearest block-level container
  const blockTags = new Set(["P", "LI", "TD", "DT", "DD", "BLOCKQUOTE", "DIV"]);
  let block: Element | null = a.parentElement;
  while (block && !blockTags.has(block.tagName)) {
    block = block.parentElement;
  }
  if (!block) return undefined;

  const fullText = block.textContent?.replace(/\s+/g, " ").trim() ?? "";
  if (!fullText || fullText === linkText) return undefined;

  // Split into sentences and find the one containing the link text
  const sentences = fullText.match(/[^.!?]+[.!?]*/g) ?? [fullText];
  const containing = sentences.find((s) => s.includes(linkText));
  const result = (containing ?? fullText).trim();

  // Only use if it adds context beyond just the link text itself
  if (result === linkText) return undefined;
  // Truncate at 200 chars
  return result.length > 200 ? result.slice(0, 200).replace(/\s\S*$/, "") + "…" : result;
}

function collectLinks(
  articleEl: Element,
  currentPath: string,
  meta: Meta[] | null,
): ResolvedLink[] {
  const seen = new Set<string>();
  const results: ResolvedLink[] = [];

  // Exclude links inside nav/toc/pagination/the module itself
  const excluded = articleEl.querySelectorAll(
    "nav, header, footer, aside, .theme-doc-toc-desktop, " +
    ".pagination-nav, .breadcrumbs, [class*='sidebar'], " +
    "[class*='toc'], [class*='pagination'], [class*='next-steps']",
  );
  const excludedSet = new Set(Array.from(excluded));

  for (const a of Array.from(articleEl.querySelectorAll("a[href]"))) {
    // Skip if inside an excluded region
    let node: Element | null = a;
    let skip = false;
    while (node) {
      if (excludedSet.has(node)) { skip = true; break; }
      node = node.parentElement;
    }
    if (skip) continue;

    const raw = a.getAttribute("href") ?? "";

    // Only internal content links + allowed external (GitHub, move-book)
    if (!isInternalContentLink(raw)) continue;

    const external = isExternal(raw);
    const absolute = external ? raw : normalizePath(raw);
    const key = absolute.split("#")[0];

    if (seen.has(key)) continue;
    if (!external && key === normalizePath(currentPath)) continue;
    seen.add(key);

    let title: string;
    let description: string;

    if (external) {
      const linkText = a.textContent?.trim();
      const githubFile = parseGithubFilePath(raw);
      if (githubFile) {
        title = `GitHub: ${githubFile.repo}`;
        description = githubFile.path;
      } else {
        title = toTitleCase((linkText && linkText.length > 0) ? linkText : humanize(raw));
        description = extractSurroundingText(a) ?? title;
      }
    } else {
      const resolved = resolveMeta(absolute, meta);
      title = toTitleCase(resolved?.title ?? humanize(absolute));
      description = resolved?.description ?? extractSurroundingText(a) ?? title;
    }

    results.push({ href: absolute, title, description, external });
  }

  return results;
}

// ── Card component ────────────────────────────────────────────────────────────

function LinkCard({ link }: { link: ResolvedLink }) {
  const inner = (
    <>
      <div className="card__header">{link.title}</div>
      {link.description && (
        <div className="card__copy">{link.description}</div>
      )}
    </>
  );

  return link.external ? (
    <a href={link.href} rel="noopener noreferrer" target="_blank">{inner}</a>
  ) : (
    <Link to={link.href}>{inner}</Link>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

interface AutoRelatedLinksProps {
  /** Override the section heading. Defaults to "Related topics". */
  title?: string;
  /** Override the section description. */
  description?: string;
  /** Maximum number of links to show. Defaults to 6. */
  maxLinks?: number;
  /**
   * CSS selector for the article container that the module is injected into.
   * Defaults to "article .theme-doc-markdown" which is the Docusaurus main
   * content div — placing the module inside the content column so the TOC
   * renders alongside it correctly.
   */
  contentSelector?: string;
}

export default function AutoRelatedLinks({
  title = "Related topics",
  description,
  maxLinks = 6,
  contentSelector = "article .theme-doc-markdown",
}: AutoRelatedLinksProps) {
  const meta = useMetaSafe();
  const { pathname } = useLocation();
  const [links, setLinks] = useState<ResolvedLink[]>([]);
  const [portalTarget, setPortalTarget] = useState<Element | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const timer = setTimeout(() => {
      const article = document.querySelector("article");
      const content = document.querySelector(contentSelector);
      if (!article || !content) return;

      // Skip if the page has a DocCardList or hand-authored next-steps-module
      if (shouldSkipPage()) return;

      const collected = collectLinks(article, pathname, meta);
      setLinks(collected.slice(0, maxLinks));

      // Create or reuse a mount point appended inside the content div
      // so the module sits at the end of the markdown body, inside the
      // content column, letting the TOC wrap alongside it.
      let mount = content.querySelector<HTMLElement>(".auto-related-links-mount");
      if (!mount) {
        mount = document.createElement("div");
        mount.className = "auto-related-links-mount";
        content.appendChild(mount);
      }
      containerRef.current = mount as HTMLDivElement;
      setPortalTarget(mount);
    }, 100);

    return () => {
      clearTimeout(timer);
      // Clean up mount point on route change
      containerRef.current?.remove();
      containerRef.current = null;
      setPortalTarget(null);
    };
  }, [pathname, meta, maxLinks, contentSelector]);

  if (links.length === 0 || !portalTarget) return null;

  return createPortal(
    <div className="next-steps-module">
      <div className="next-steps-header">
        <h3>{title}</h3>
      </div>
      {description && (
        <p className="next-steps-description">{description}</p>
      )}
      <div className="next-steps-grid">
        {links.map((link) => (
          <LinkCard key={link.href} link={link} />
        ))}
      </div>
    </div>,
    portalTarget,
  );
}