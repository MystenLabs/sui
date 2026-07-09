# Prompt: Integrate PlayMove + CodeWalkthrough into a Docusaurus Site

Use this prompt to replicate the PlayMove interactive editor and CodeWalkthrough scroll-spy component in another Docusaurus documentation site.

---

## What you're building

Two features for a Docusaurus site that displays Move smart contract code:

1. **PlayMoveEmbed** — Replaces static Move code blocks with an embedded PlayMove IDE (via iframe to `https://www.playmove.dev`). Users can edit, build, and test Move code directly in the docs. Includes Copy and Open in Agent (Claude/ChatGPT/Gemini) buttons in a toolbar.

2. **CodeWalkthrough** — A split-view scroll-spy component where explanations scroll on the left while the full source code stays sticky on the right. As the reader scrolls through each explanation step, the corresponding lines in the code panel highlight and the rest dim. Inspired by Stripe/Solana docs patterns.

---

## Feature 1: PlayMoveEmbed

### Create `site/src/shared/components/PlayMoveEmbed/index.tsx`

A React component that:
- Accepts `code` (string), `title` (optional string), and `height` (optional string, default `"600px"`) props
- Renders an iframe to `https://www.playmove.dev/?theme={light|dark}#{encodeURIComponent(code)}`
- Detects the Docusaurus site theme via `document.documentElement.getAttribute("data-theme")` and passes it as a `?theme=` query param
- Wraps in `BrowserOnly` from `@docusaurus/BrowserOnly` for SSR safety
- Includes a toolbar above the iframe with:
  - Title (filename) on the left
  - **Copy button** using `copy-text-to-clipboard` (already a Docusaurus dependency)
  - **Open in Agent dropdown** with Claude, ChatGPT, Gemini links that open `https://{agent}/?q={encoded prompt}` in a new tab
- Includes a hidden `<pre><code className="language-move">{code}</code></pre>` for external tooling that walks the DOM
- The iframe should have `allow="clipboard-write"` and `sandbox="allow-scripts allow-same-origin allow-popups allow-forms"`

### Create `site/src/shared/components/PlayMoveEmbed/styles.css`

Style the component using Docusaurus CSS variables for theme compatibility:
- `.playmove-embed` — container with `border-radius: var(--ifm-code-border-radius)`, `overflow: hidden`, `border: 1px solid var(--ifm-color-emphasis-200)`, `background: var(--prism-background-color)`
- `.playmove-toolbar` — flexbox row with `justify-content: space-between`, matching code block header styling
- `.playmove-toolbar-btn` — small buttons with `font-size: 12px`, transparent background, hover state using `var(--ifm-color-emphasis-200)`
- `.playmove-agent-dropdown` — absolute-positioned dropdown with `z-index: 100`, shadow, rounded corners
- `.playmove-iframe` — `border: none; display: block; background: #1e1e1e`
- Add `[data-theme="light"]` overrides for light mode borders/backgrounds

Import `./styles.css` at the top of the component file.

### Modify `ImportContent` to use PlayMoveEmbed for Move code

In your site's ImportContent component (the component that renders source code from files), add a conditional just before the final `<CodeBlock>` return:

```tsx
import PlayMoveEmbed from "@site/src/shared/components/PlayMoveEmbed";

// ... at the end of the render, before the CodeBlock return:
if (resolvedLanguage === "move") {
  return <PlayMoveEmbed code={out} title={noTitle ? undefined : title} />;
}

return (
  <CodeBlock language={resolvedLanguage} metastring={meta}>
    {out}
  </CodeBlock>
);
```

This automatically converts all Move code imports to interactive PlayMove editors with zero MDX changes.

---

## Feature 2: CodeWalkthrough

### Create `site/src/shared/components/CodeWalkthrough/index.tsx`

A React component that:
- Accepts props: `source` (file path), `org`/`repo`/`branch` (optional, for GitHub), `language` (optional), and `children` (Step elements)
- Exports a `Step` subcomponent: `export function Step(_props: { lines: string; title?: string; children: React.ReactNode }) { return null; }`
  - `Step` is declarative only — it renders nothing. The parent reads its props via `React.Children.forEach`
- Fetches code from either GitHub raw (`https://raw.githubusercontent.com/{org}/{repo}/{branch}/{source}`) or a local import map
- Strips the standard copyright header (`// Copyright (c) ... / // SPDX-License-Identifier: ...`) and leading blank lines
- Renders a two-column layout:
  - **Left (50%)**: Scrollable step cards. Each step has a title (h4), content (children), and a left border that turns blue when active
  - **Right (50%)**: Sticky code panel with Prism syntax highlighting. Each line gets a `data-line={n}` attribute
- Uses `IntersectionObserver` on each step div (rootMargin: `-30% 0px -50% 0px`) to detect which step is in view and update `activeStep` state
- The active step's `lines` prop (e.g., `"5-9"` or `"1-3,5"`) determines which code lines get the `.cw-line--highlighted` class; all other lines get `.cw-line--dimmed` (opacity: 0.3)
- Auto-scrolls the code panel to keep highlighted lines visible using `element.scrollIntoView({ block: "nearest", behavior: "smooth" })`
- Uses `Highlight` from `prism-react-renderer` and `usePrismTheme` from `@docusaurus/theme-common` for syntax highlighting

### Create `site/src/shared/components/CodeWalkthrough/styles.css`

Key styles:
- `.cw-container` — `display: flex; gap: 1.5rem; align-items: flex-start`
- `.cw-steps` / `.cw-code-panel` — both `flex: 1 1 50%`
- `.cw-code-sticky` — `position: sticky; top: 80px; max-height: calc(100vh - 100px); overflow: auto`; styled like a code block with `border-radius`, `border`, `background: var(--prism-background-color)`
- `.cw-step` — `padding: 1.25rem 1rem; border-left: 3px solid transparent; cursor: pointer`
- `.cw-step--active` — `border-left-color: var(--ifm-color-primary); background: var(--ifm-color-emphasis-100)`
- `.cw-line--highlighted` — `background: var(--docusaurus-highlighted-code-line-bg)` (or `rgba(255,255,255,0.08)` in dark mode)
- `.cw-line--dimmed` — `opacity: 0.3`
- `.cw-line` — `transition: opacity 0.2s ease, background-color 0.2s ease` for smooth highlight changes
- `.cw-line-number` — `width: 2.5rem; text-align: right; color: var(--ifm-color-emphasis-400); user-select: none`
- `@media (max-width: 768px)` — collapse to single column, code panel on top (not sticky)

### Register components globally in MDXComponents

In `site/src/theme/MDXComponents/index.jsx`:

```jsx
import CodeWalkthrough, { Step } from "@site/src/shared/components/CodeWalkthrough";

export default {
  ...MDXComponentsOriginal,
  // ... existing components
  CodeWalkthrough,
  Step,
};
```

### MDX usage pattern

```mdx
<CodeWalkthrough
  source="/path/to/file.move"
  org="MyOrg"
  repo="my-repo"
  language="move"
>
  <Step lines="1-5" title="Module declaration">
    Explanation of the module header and imports.
  </Step>
  <Step lines="7-12" title="Struct definition">
    Explanation of the struct and its abilities.
  </Step>
  <Step lines="14-22" title="Constructor function">
    Explanation of how the constructor works.
  </Step>
</CodeWalkthrough>
```

The `lines` prop uses 1-based line numbers from the **cleaned** code (after license header stripping). Supports ranges (`"5-9"`) and comma-separated values (`"1-3,5,8-10"`).

---

## Identifying pages to convert

Good candidates for CodeWalkthrough are pages that:
1. Show a full source file via an import/code block
2. Then explain it piece by piece (struct by struct, function by function) with separate code blocks for each piece

**Not suitable** for CodeWalkthrough:
- Pages that mix imports from multiple different source files in the same explanation flow
- Pages with mermaid diagrams interleaved between code sections (the diagrams break the two-column flow)
- Pages where code is inside `<details>` disclosure blocks
- Pages with only 1 code section (nothing to scroll through)

---

## Dependencies

These are standard Docusaurus dependencies (no new packages needed):
- `prism-react-renderer` (for CodeWalkthrough syntax highlighting)
- `@docusaurus/theme-common` (for `usePrismTheme`)
- `@docusaurus/BrowserOnly` (for SSR safety in PlayMoveEmbed)
- `copy-text-to-clipboard` (for the copy button)
- `IntersectionObserver` API (browser-native, no polyfill needed)

---

## Verification checklist

1. Start the dev server (`pnpm start` or `npx docusaurus start`)
2. Navigate to a page with Move `ImportContent` blocks — confirm they render as PlayMove iframes
3. Confirm non-Move code blocks (TypeScript, Rust, etc.) still render as static CodeBlock
4. Navigate to a page with `<CodeWalkthrough>` — confirm the split-view layout
5. Scroll through steps — confirm code highlights update smoothly
6. Test the Copy button and Open in Agent dropdown
7. Toggle dark/light mode — confirm the toolbar and code panel adapt
8. Check responsive layout at mobile width — confirm single-column fallback
9. Run `pnpm build` — confirm production build succeeds
