# `move prompt`

`move prompt` is the agent-agnostic entry point to expert Move knowledge inside
`move-cli`: a self-contained, embedded source of skills and workflows organized into
**categories**
(`audit`, `bytecode`, …). Each category is a small workflow that points an AI agent (or a
human) at the skills relevant to a kind of work.

This README is the developer-facing entry point for the subcommand. The text printed at
runtime by `move prompt` (no args) lives in `prompt-output.md` next to this file and is
embedded into the binary at build time via `include_str!`.

## What this subcommand is

- **Contract:** out = markdown (categories, skills, overview). Read-only — never builds,
  fetches, or writes artifacts.
- **Self-contained.** All category and skill markdown is embedded in the binary at build
  time. No external install needed.
- **Agent-agnostic.** Works for any AI agent that can shell out.

## Install

Build from source:

```sh
cd /path/to/sui-move-prompt/external-crates/move
cargo build --release -p move-cli --bin move
./target/release/move prompt   # see the overview
```

Or distribute the built binary; no additional install steps are needed.

## Commands

### Example 1 — discoverability overview

```sh
move prompt
```

Prints `prompt-output.md`, embedded at build time. The overview names the categories and
explains how to navigate to them. An agent reads this first to learn the surface.

### Example 2 — list categories

```sh
move prompt categories
```

Output:

```
Embedded categories (2):
  audit — Auditing compiled Sui Move packages for security vulnerabilities.
  bytecode — Understanding compiled Move bytecode — disassembly, decompilation, and what survives compilation.

Commands:
  move prompt category <name>   — read the category's content
  move prompt skills            — list all skill bundles (flat)
  move prompt skill <bundle>    — read a skill bundle's SKILL.md
```

### Example 3 — read a category

```sh
move prompt category audit
```

Prints the body of `categories/audit/CATEGORY.md` verbatim — the workflow, the skill
references in order, the triage discipline, and external references. The category body is
where an agent learns *how to do this kind of work*.

### Example 4 — list skill bundles (flat)

```sh
move prompt skills
```

Output:

```
Embedded skill bundles (3):
  move-bytecode-comprehension  (4 files)
  sui-and-move-tools  (3 files)
  sui-move-security-review  (12 files)

Commands:
  move prompt skill <bundle>            — read SKILL.md
  move prompt skill <bundle> --list     — list reference files
  move prompt skill <bundle> --file <r> — read a specific reference file
```

### Example 5 — read a skill bundle's SKILL.md

```sh
move prompt skill sui-move-security-review
```

SKILL.md is the bundle's routing table. Reading it alone is not enough — drill into the
reference files for the actual content.

### Example 6 — list reference files in a bundle

```sh
move prompt skill sui-move-security-review --list
```

### Example 7 — read a specific reference file

```sh
move prompt skill sui-move-security-review --file access-control
```

## Worked agent flow

Realistic use case — point any AI agent at the binary with a prompt that names the kind
of work (e.g. *"audit Sui mainnet package `0x<id>` for security vulnerabilities; use the
`move prompt` binary on PATH to find the right skills"*):

1. Agent calls `move prompt` — learns the surface.
2. Agent calls `move prompt categories` — sees `audit` and `bytecode`.
3. Agent calls `move prompt category audit` — gets the workflow + the ordered skill list
   + triage discipline + external references.
4. For each skill the category names, the agent calls `move prompt skill <bundle>`,
   `move prompt skill <bundle> --list`, then `move prompt skill <bundle> --file <ref>`
   for every reference file before applying the skill's rules.
5. Agent follows the workflow against the target package — fetch the `.mv` modules,
   `sui move disassemble` them, walk the SM-* rules, etc.
6. Agent produces findings in the format the audit category prescribes:
   `SM-ID · module.asm:B<block>@i<index>` with paired disassembly evidence + a decompiled
   "Human view" excerpt.

The same shape applies to other categories: read the category's body, walk the skills it
names, do the work.

## How to add a category

A category is a single markdown file plus, optionally, references to existing skills:

1. Create `src/prompt/categories/<name>/CATEGORY.md`.
2. Add YAML frontmatter with the required keys:
   ```yaml
   ---
   name: <name>
   description: <one-line description used by `move prompt categories`>
   skills:
     - <skill-bundle-1>
     - <skill-bundle-2>
   ---
   ```
   The `skills:` list names skill bundles (directories under `src/prompt/skills/`). A
   skill can appear in any number of categories — there's no duplication, the
   bundle's canonical location stays under `src/prompt/skills/`.
3. Write the body: a workflow walking through the skills in order, optionally with a
   "Discipline" or "Reproducibility" section, and an "External references" section if
   useful. Describe what is — not what's planned.
4. Rebuild: `cargo build --release -p move-cli --bin move`. The build script picks up
   the new category automatically.
5. Verify: `move prompt categories` lists the new entry; `move prompt category <name>`
   prints the body.

## How to add a skill bundle

1. Create `src/prompt/skills/<bundle>/SKILL.md` (the bundle's entry point) plus any
   per-topic reference files (`<topic>.md`).
2. Optionally reference the new bundle from one or more `CATEGORY.md` frontmatter
   `skills:` lists.
3. Rebuild.
