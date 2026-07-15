# Sui Documentation

This directory contains the source for [docs.sui.io](https://docs.sui.io). It is split between `content/` (documentation pages) and `site/` (Docusaurus configuration, plugins, and scripts).

## Repository layout

```
docs/
├── content/                    # All documentation source files (.mdx)
│   ├── getting-started/        # Onboarding, examples, migration guides
│   ├── develop/                # Move, objects, transactions, data access, testing
│   ├── onchain-finance/        # Tokens, DeepBook, kiosk, payments, asset custody
│   ├── operators/              # Full nodes, validators, data management
│   ├── references/             # CLI, APIs, framework, SDKs, contributing
│   ├── sui-stack/              # Walrus, Seal, zkLogin, Nautilus, SuiNS, Enoki
│   ├── snippets/               # Reusable content referenced by <ImportContent>
│   └── sidebars.js             # Navigation structure / page hierarchy
├── site/                       # Docusaurus site
│   ├── src/                    # Custom components, plugins, utilities
│   ├── scripts/                # Build, audit, and goal generation scripts
│   ├── docusaurus.config.js
│   └── package.json
├── concept-map.yaml            # Concept coverage definitions for audit pipeline
└── README.md
```

## Page frontmatter

Every `.mdx` page has YAML frontmatter. Required fields:

```yaml
---
title: Page Title
description: One-sentence summary of what this page covers.
keywords: [keyword1, keyword2, keyword3]
---
```

### Goal frontmatter

Pages also have a `goal:` block that defines what the page should achieve for the reader and a set of mechanically verifiable checks:

```yaml
goal:
  description: Reader can build, publish, and call a Move package
  requires:
    - pattern: 'sui move build'
      min: 1
      label: Shows build command
    - pattern: '```move'
      min: 1
      label: Has Move source code
    - headings:
        - pattern: Build
        - pattern: Publish
      label: Has build and publish sections
    - min_words: 300
      label: Needs more walkthrough depth
    - has_frontmatter:
        - title
        - description
        - keywords
      label: Has required frontmatter fields
```

**Available check types:**

| Check | Parameters | What it verifies |
|-------|-----------|-----------------|
| `pattern` | `pattern` (regex), `min` (count) | Body text matches the pattern at least `min` times |
| `headings` | Array of `{ pattern }` | Page has headings matching each pattern |
| `links_to` | Array of paths | Page contains links to specific internal paths |
| `has_tables` | `min` (count) | Page has at least `min` markdown tables |
| `has_images` | boolean | Page has (or doesn't have) images |
| `has_frontmatter` | Array of field names | Specified frontmatter fields are present |
| `min_words` | number | Page has at least this many words (outside code blocks) |
| `has_questions` | boolean | Page has `questions:` frontmatter for AI search |
| `has_answer` | boolean | Page has `answer:` frontmatter for AI citation |
| `answer_in_intro` | number (min words) | First paragraph has enough words to serve as a direct answer |
| `question_headings` | number (min count) | Headings use question format (What/How/Why) |
| `steps_present` | number (min count) | Page has numbered steps for procedural content |
| `code_explanation_ratio` | number (min ratio) | Ratio of explanation to code is above threshold |

Goals are evaluated by the audit pipeline. The label appears in failure reports, so write it to describe what's wrong (e.g., "Needs more content depth" not "Sufficient content depth").

### GEO/AEO frontmatter

Pages have `questions:` and `answer:` fields for Generative Engine Optimization (GEO) and Answer Engine Optimization (AEO). These help AI-powered search engines (Perplexity, ChatGPT, Google AI Overviews) surface and cite the page correctly.

```yaml
questions:
  - How do I install the Sui CLI?
  - What is suiup?
  - How do I verify my Sui installation?
answer: >-
  Run `curl -sSfL https://raw.githubusercontent.com/MystenLabs/suiup/main/install.sh | sh`
  to install suiup, then `suiup install sui@testnet` for the Testnet toolchain.
  Verify with `sui --version`.
```

- **`questions`**: 2-5 questions this page answers. AI engines match user queries against these.
- **`answer`**: 1-2 sentence direct answer to the page's primary question. This is what an AI would cite verbatim.

Generate these for new pages:

```sh
cd docs/site && node scripts/generate-geo.mjs --apply
```

### Builder path frontmatter

Pages that belong to a builder path (DeFi, Payments, Walrus, etc.) are tagged with `builder_paths:`:

```yaml
builder_paths:
  - path_id: defi-deepbook
    path_name: DeFi / DeepBook
    step: Custom coin creation
    stage: Move Contract
    eval: covered            # covered | partial | missing (from evals dashboard)
  - path_id: p2p-payments
    path_name: P2P Payments
    step: Stablecoin integration
    stage: Tokens
    # no eval = not yet evaluated
```

Pages with `eval:` have been scored in the [evals dashboard](https://docs-analytics-dashboard.vercel.app/evals). Pages without `eval` are identified as belonging to a path but haven't been scored yet.

## Audit pipeline

The docs audit runs deterministic checks across all pages. Three layers:

1. **Base checks** -- frontmatter completeness, staleness (git log), broken internal links, broken imports, unclosed code fences, TODO/FIXME markers, word count, missing images, duplicate titles.
2. **Goal checklist** -- evaluates each page's `goal.requires` checks.
3. **Concept coverage** -- cross-references pages against `concept-map.yaml` to find coverage gaps and orphan pages.

### Running the audit

```sh
cd docs/site
pnpm audit              # JSON to stdout
pnpm audit:summary      # human-readable summary to stderr
pnpm audit:failures     # only failing pages + summary
```

### Generating goals for new pages

When you add new `.mdx` pages, generate goal frontmatter:

```sh
cd docs/site
node scripts/generate-goals.mjs --apply
```

This detects each page's archetype (onboarding, example, guide, reference, operator, SDK, index) and generates appropriate checks. Review and adjust the generated goals before committing.

### CI enforcement

A GitHub Actions workflow (`docs-frontmatter-check.yml`) runs on every PR that touches `docs/content/**/*.mdx`. It comments on the PR listing any pages missing `title`, `description`, `keywords`, or `goal` frontmatter, with instructions to auto-generate.

## Build the site locally

```sh
cd docs/site
pnpm install
```

### Full build

```sh
pnpm build
```

A full build downloads spec files, generates reference docs, and compiles the static site. Required after a fresh clone. The build fails on broken links and missing imports.

Build steps:
1. Fetch external docs and generate import context
2. Download gRPC specs, generate GraphQL reference docs
3. Download OpenRPC specs, post-process GraphQL output
4. Run `docusaurus build` into `site/build`
5. Generate `llms.txt` and `llms-full.txt` for LLM consumption
6. Run internal link checking

### Development preview

```sh
pnpm start
```

Starts a dev server at `localhost:3000` with hot reload. Run `pnpm build` before submitting -- the full build catches errors the dev preview skips.

## Auto-generated content

Do not edit these sections directly:

- **Framework reference** (`/references/framework`) -- generated from `cargo-doc` Markdown in `/sui/crates`.
- **GraphQL reference** (`/references/sui-api/sui-graphql`) -- generated from the GraphQL schema.
- **OpenRPC and gRPC specs** -- downloaded during build.

## Scripts

Key scripts in `docs/site/scripts/`:

| Script | Purpose |
|--------|---------|
| `audit-docs.mjs` | Deterministic docs audit pipeline |
| `generate-goals.mjs` | Generate goal frontmatter by page archetype |
| `generate-geo.mjs` | Generate questions + answer frontmatter for GEO/AEO |
| `add-builder-paths.mjs` | Map pages to builder paths with eval status |
| `refine-goals.mjs` | Batch refinement of goal descriptions and checks |
| `build-and-check.sh` | Full build + link checking (called by `pnpm build`) |
| `generate-import-context.js` | Resolve `<ImportContent>` source paths |
| `fetch-external-docs.js` | Download docs from external repos |

## For AI agents and LLMs

- **`llms.txt`**: Generated at `https://docs.sui.io/llms.txt`. Use as entry point for documentation structure.
- **Style guide skill**: Machine-readable style rules at `docs/sui-documentation-style-guide.skill`.
- **`sidebars.js`**: Full navigation tree at `docs/content/sidebars.js`.
- **`mdx-components.mdx`**: Custom component reference at `docs/content/references/contribute/mdx-components.mdx`.

## Pull requests

Vercel builds a preview for every PR. Find the **Visit Preview** link in the PR comments from the Vercel bot.

To preview before your changes are ready for review, [mark your PR as a draft](https://github.blog/2019-02-14-introducing-draft-pull-requests/).

## Style guide

All contributions must follow the [Sui Documentation Style Guide](https://docs.sui.io/references/contribute/style-guide):

- US English, active voice, present tense, second person ("you")
- No Latin abbreviations (use "for example" not "e.g.")
- Serial (Oxford) commas
- Sentence case for headings

## Contributing

- [Contribution process](https://docs.sui.io/references/contribute/contribution-process)
- [Repo contributing guidelines](https://docs.sui.io/references/contribute/contribute-to-sui-repos)
- [Style guide](https://docs.sui.io/references/contribute/style-guide)
- [MDX components](https://docs.sui.io/references/contribute/mdx-components)
- [Code of conduct](https://docs.sui.io/references/contribute/code-of-conduct)

## License

The Sui documentation is distributed under the [CC BY 4.0 license](../LICENSE-docs).
