# move-prompt skills

Skill bundles embedded into the `move` binary at build time. Each bundle is a directory
holding a `SKILL.md` (the routing/summary entry point) and one or more reference files
(`<topic>.md`) that hold the actual content. Skills are the unit of expert content; they
are organized by use case via **categories** (see `../categories/` and
`move prompt categories`).

A skill can belong to more than one category — categories reference skills by name, and
skills live here in one canonical location.

## Bundles

- **`sui-move-security-review`** — the SM-* rule catalog. Each rule has an invariant, a
  disassembly detection signal, a severity, and an exploit sketch. Includes
  `auditing-bytecode.md` (per-rule disassembly signals + the report format that pairs
  assembly evidence with a human view).
- **`sui-and-move-tools`** — tools for working with Sui Move: standing up the
  `suiup`-managed `sui` CLI for fetching and disassembling on-chain bytecode, and the
  `move decompile` binary for human-readable rendering. Includes the confirm-`SUI_REF`
  gate and the isolation rule that forbids local Sui checkouts.
- **`move-bytecode-comprehension`** — Move binary format essentials + the survival table
  (what survives compilation), plus how to read disassembly and decompiled output without
  being misled by decompiler artifacts.

## Access (any AI agent)

```sh
move prompt skills                                                # list bundles
move prompt skill <bundle>                                        # read SKILL.md
move prompt skill <bundle> --list                                 # list reference files
move prompt skill <bundle> --file <ref>                           # read a reference file
```

Categories (`move prompt categories` / `move prompt category <name>`) guide the agent
through a workflow that names the right skills in order; direct skill access stays
available when the category context isn't needed.

## Lineage

The SM-* catalog in `sui-move-security-review/` was derived from the constructive
`MystenLabs/skills` "how to write correct Sui Move" guidance. See
`../maintenance/sui-move-security-review/LINEAGE.md` for the pinned upstream ref and the
refresh protocol — kept out of the embedded surface so the agent's context doesn't carry
provenance metadata.

## Editing notes

- This directory is the canonical home for skills. Files here are embedded into the
  binary at compile time by `move-cli/build.rs`. To distribute changes, rebuild the binary.
- A skill's content is agent-model-agnostic — no references to a specific AI model or
  vendor; generic "the agent" wording is the convention.

## Appendix — optional Claude Code skill auto-discovery

For users running Claude Code who prefer native skill auto-discovery (instead of pulling
each file via `move prompt skill`), the bundles in this directory can also be installed
via the standard `skills` CLI:

```sh
npx skills add path/to/move-cli/src/prompt/skills --skill '*' --agent claude-code --global -y
```

This is purely optional convenience. The primary, agent-agnostic interface is
`move prompt skill <bundle>` — that's how the surface is designed to work for any agent.
