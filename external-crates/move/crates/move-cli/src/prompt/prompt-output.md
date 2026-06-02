# move prompt — entry point to expert Move knowledge

`move prompt` is the entry point — for AI agents and humans alike — to a self-contained,
agent-agnostic source of expert Move knowledge. Content is organized into **categories**.
Each category bundles the skills relevant to a kind of work — auditing a deployed package,
reading its bytecode, etc. — and a category preamble walks you (or your AI agent) through
that workflow.

## Contract

- **Out:** markdown only (categories, skills, overview).
- **Read-only:** never builds, fetches, or writes artifacts.
- **Self-contained:** every category and skill is embedded in this binary at build time.
  No external install needed.
- **Agent-agnostic:** works for any AI agent that can shell out.

## How to use this

Start with a category. The category's body lists which skills to read, in what order, and
how to chain them. Each skill is a two-tier bundle — a `SKILL.md` that routes/summarizes
plus reference files that hold the actual content; you should `--list` and read every
reference file before applying the skill.

```sh
move prompt categories                    # see the available categories
move prompt category <name>               # read a category's workflow + skill list
```

You can also reach skills directly when their category context isn't needed:

```sh
move prompt skills                        # list all skill bundles, flat
move prompt skill <bundle>                # read a bundle's SKILL.md
move prompt skill <bundle> --list         # enumerate the bundle's reference files
move prompt skill <bundle> --file <ref>   # read a specific reference file
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

If still unsure, run `move prompt categories` and read every description before
picking — the descriptions are written with common intents in mind, not just the
literal category name.

## Universal commands

- `move prompt`                                          — this overview
- `move prompt categories`                               — list categories
- `move prompt category <name>`                          — read a category's content
- `move prompt skills`                                   — list skill bundles (flat)
- `move prompt skill <bundle>`                           — read a bundle's `SKILL.md`
- `move prompt skill <bundle> --list`                    — list reference files in a bundle
- `move prompt skill <bundle> --file <ref>`              — read a specific reference file

## Adding categories and skills

The project is designed to grow. Categories and skill bundles live under
`move-cli/src/prompt/categories/` and `move-cli/src/prompt/skills/`; adding either is a
markdown-only change picked up by the build script. See `move-cli/src/prompt/README.md`
for the procedure.
