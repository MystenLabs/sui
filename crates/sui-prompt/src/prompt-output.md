# sui prompt — entry point to expert Move knowledge

`sui prompt` is the entry point — for AI agents and humans alike — to a self-contained,
agent-agnostic source of expert Move knowledge. Content is organized into **categories**.
Each category bundles the skills relevant to a kind of work — auditing a deployed package,
reading its bytecode, etc. — and a category preamble walks you (or your AI agent) through
that workflow.

## Contract

- **Out:** markdown only (categories, skills, overview).
- **Read-only command:** `sui prompt` only prints embedded markdown; it never builds,
  fetches, or writes artifacts. Some workflows may instruct follow-up commands when the
  task requires them.
- **Self-contained:** every category and skill is embedded in this binary at build time.
  No external install needed.
- **Agent-agnostic:** works for any AI agent that can shell out.

## How to use this

Start with a category. The category's body lists which skills to read, in what order, and
how to chain them. Each skill is a two-tier bundle — a `SKILL.md` that routes/summarizes
plus reference files that hold the actual content; **read every reference file** before
applying the skill. `--all` loads them in one call (default whenever your context allows);
`--list` + per-file reads is the alternative if you need to budget context tighter.

```sh
sui prompt categories                    # see the available categories
sui prompt category <name> --list        # list bundle and reference file names (no content)
sui prompt category <name>               # read a category's workflow + skill list
sui prompt category <name> --all         # read every bundle's content in one call
```

You can also reach skills directly when their category context isn't needed:

```sh
sui prompt skills                        # list all skill bundles, flat
sui prompt skill <bundle>                # read a bundle's SKILL.md
sui prompt skill <bundle> --list         # list reference file names (no content)
sui prompt skill <bundle> --file <ref>   # read a specific reference file
sui prompt skill <bundle> --all          # read SKILL.md + every reference file
```

A skill can belong to more than one category; reaching it directly is fine.

## Picking a category

Match by intent, not by exact word. The user is unlikely to ask for "an audit" or "a
bytecode analysis" verbatim; pattern-match on what they want to *achieve*.

- The user wants to know whether a deployed package is **safe**, has **bugs** or
  **vulnerabilities**, behaves **incorrectly**, **suspects something is wrong**, or asks
  for a **security review** / **audit** → **`audit`** is the entry point.
- The user wants to **read** what a deployed package actually does, **decompile** it,
  understand its **bytecode**, or inspect a `.mv` without a security framing →
  **`bytecode`** is the entry point.

If still unsure, run `sui prompt categories` and read every description before
picking — the descriptions are written with common intents in mind, not just the
literal category name.

## Universal commands

- `sui prompt`                                          — this overview
- `sui prompt categories`                               — list categories
- `sui prompt category <name> --list`                   — list bundle and reference file names (no content)
- `sui prompt category <name>`                          — read a category's content
- `sui prompt category <name> --all`                    — read every bundle's content in one call
- `sui prompt skills`                                   — list skill bundles (flat)
- `sui prompt skill <bundle>`                           — read a bundle's `SKILL.md`
- `sui prompt skill <bundle> --list`                    — list reference file names (no content)
- `sui prompt skill <bundle> --file <ref>`              — read a specific reference file
- `sui prompt skill <bundle> --all`                     — read `SKILL.md` + every reference file
