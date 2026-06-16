# `sui prompt`

`sui prompt` is the agent-agnostic entry point to expert Move knowledge, shipped as
part of the Sui CLI: a self-contained, embedded source of skills and workflows
organized into **categories** (`audit`, `bytecode`, …). Each category is a small
workflow that points an AI agent (or a human) at the skills relevant to a kind of
work.

This README is the developer-facing entry point for the subcommand. The text printed at
runtime by `sui prompt` (no args) lives in `src/prompt-output.md` and is embedded into
the binary at build time via `include_str!`.

## What this subcommand is

- **Contract:** out = markdown (categories, skills, overview). `sui prompt` itself only
  prints embedded markdown; it never builds, fetches, or writes artifacts. Some workflows
  may instruct follow-up commands when the task requires them.
- **Self-contained.** All category and skill markdown is embedded in the binary at
  build time.
- **Agent-agnostic.** Works for any AI agent that can shell out.

## Install

`sui prompt` is built into the Sui CLI. Install `sui` per the [official Sui CLI install
guide](https://docs.sui.io/getting-started/onboarding/sui-install); once `sui` is on
your `PATH`, `sui prompt` is available.

## Commands

### Example 1 — discoverability overview

```sh
sui prompt
```

Prints `prompt-output.md`, embedded at build time. The overview names the categories and
explains how to navigate to them. An agent reads this first to learn the surface.

### Example 2 — list categories

```sh
sui prompt categories
```

Prints a Markdown list of embedded categories. Each entry includes the category name and
the short frontmatter description used for routing. The list is followed by navigation
commands for reading a category or switching to skill-bundle navigation.

### Example 3 — read a category

```sh
sui prompt category audit
```

Prints the body of `categories/audit/CATEGORY.md` verbatim — the workflow, the skill
references in order, the triage discipline, and external references. The category body is
where an agent learns *how to do this kind of work*.

### Example 4 — list skill bundles (flat)

```sh
sui prompt skills
```

Prints a Markdown list of embedded skill bundles. Each entry includes the bundle name and
the number of embedded markdown files. The list is followed by commands for reading the
bundle entry point, listing reference files, or reading a specific reference file.

### Example 5 — read a skill bundle's SKILL.md

```sh
sui prompt skill sui-move-security-review
```

SKILL.md is the bundle's routing table. Reading it alone is not enough — drill into the
reference files for the actual content.

### Example 6 — list reference files in a bundle

```sh
sui prompt skill sui-move-security-review --list
```

### Example 7 — read a specific reference file

```sh
sui prompt skill sui-move-security-review --file access-control
```

## Worked agent flow

Realistic use case — point any AI agent at the binary with a prompt that names the kind
of work (e.g. *"audit Sui mainnet package `0x<id>` for security vulnerabilities; use the
`sui prompt` binary on PATH to find the right skills"*):

1. Agent calls `sui prompt` — learns the surface.
2. Agent calls `sui prompt categories` — sees `audit` and `bytecode`.
3. Agent calls `sui prompt category audit` — gets the workflow + the ordered skill list
   + triage discipline + external references.
4. For each skill the category names, the agent calls `sui prompt skill <bundle>`,
   `sui prompt skill <bundle> --list`, then `sui prompt skill <bundle> --file <ref>`
   for every reference file before applying the skill's rules.
5. Agent follows the workflow against the target package — fetch via one Sui GraphQL
   call, decompile, walk the SM-* rules over the decompiled `.move` files.
6. Agent produces findings in the format the audit category prescribes:
   `SM-ID · module.move:<line>` with a decompiled excerpt as evidence (disassembly
   added only when verification required it).

The same shape applies to other categories: read the category's body, walk the skills it
names, do the work.

## How to add a category

A category is a single markdown file plus, optionally, references to existing skills:

1. Create `src/categories/<name>/CATEGORY.md`.
2. Add YAML frontmatter with the required keys:
   ```yaml
   ---
   name: <name>
   description: <one-line description used by `sui prompt categories`>
   skills:
     - <skill-bundle-1>
     - <skill-bundle-2>
   ---
   ```
   The `skills:` list names skill bundles (directories under `src/skills/`). A
   skill can appear in any number of categories — there's no duplication, the
   bundle's canonical location stays under `src/skills/`.
3. Write the body: a workflow walking through the skills in order, optionally with a
   "Discipline" or "Reproducibility" section, and an "External references" section if
   useful. Describe what is — not what's planned.
4. Rebuild the Sui CLI. The build script picks up the new category automatically.
5. Verify: `sui prompt categories` lists the new entry; `sui prompt category <name>`
   prints the body.

## How to add a skill bundle

1. Create `src/skills/<bundle>/SKILL.md` (the bundle's entry point) plus any
   per-topic reference files (`<topic>.md`).
2. Optionally reference the new bundle from one or more `CATEGORY.md` frontmatter
   `skills:` lists.
3. Rebuild the Sui CLI.

## Maintainer-only content

Provenance, refresh tooling, and other content that should not be visible to runtime
agents lives at `src/maintenance/` (see its own `README.md`). `build.rs` walks
only `src/skills/` and `src/categories/`, so anything under
`maintenance/` is excluded from the binary by construction.
