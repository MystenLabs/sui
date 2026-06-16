# move-prompt skills

Skill bundles embedded into the `sui` binary at build time. Each bundle is a directory
holding a `SKILL.md` (the routing/summary entry point) and one or more reference files
(`<topic>.md`) that hold the actual content. Skills are the unit of expert content; they
are organized by use case via **categories** (see `../categories/` and
`sui prompt categories`).

A skill can belong to more than one category — categories reference skills by name, and
skills live here in one canonical location.

## Bundles

- **`sui-move-security-review`** — the SM-* rule catalog. Each rule has an invariant, a
  detection heuristic, a severity, and an exploit sketch. Includes `auditing-bytecode.md`
  for applying the rules to decompiled `.move` files, with disassembly reserved for
  targeted verification.
- **`sui-and-move-tools`** — obtain bytecode + readable views for a deployed Sui package.
  One Sui GraphQL call returns every module's raw bytes; `sui move decompile` produces the
  decompiled-source working view. Disassembly is fetched per-module only for specific
  verification questions.
- **`move-bytecode-comprehension`** — Move binary format essentials + the survival table
  (what survives compilation), plus how to read disassembly and decompiled output soundly.

## Access (any AI agent)

```sh
sui prompt skills                                                # list bundles
sui prompt skill <bundle>                                        # read SKILL.md
sui prompt skill <bundle> --list                                 # list reference files
sui prompt skill <bundle> --file <ref>                           # read a reference file
```

Categories (`sui prompt categories` / `sui prompt category <name>`) guide the agent
through a workflow that names the right skills in order; direct skill access stays
available when the category context isn't needed.

## Lineage

The SM-* catalog in `sui-move-security-review/` was derived from the constructive
`MystenLabs/skills` "how to write correct Move" guidance. See
`../maintenance/sui-move-security-review/LINEAGE.md` for the pinned upstream ref and the
refresh protocol — kept out of the embedded surface so the agent's context doesn't carry
provenance metadata.

## Editing notes

- This directory is the canonical home for skills. Files here are embedded into the
  binary at compile time by `crates/sui-prompt/build.rs`. To distribute changes, rebuild the binary.
- A skill's content is agent-model-agnostic — no references to a specific AI model or
  vendor; generic "the agent" wording is the convention.

## Appendix — optional Claude Code skill auto-discovery

For users running Claude Code who prefer native skill auto-discovery (instead of pulling
each file via `sui prompt skill`), the bundles in this directory can also be installed
via the standard `skills` CLI:

```sh
npx skills add path/to/crates/sui-prompt/src/skills --skill '*' --agent claude-code --global -y
```

This is purely optional convenience. The primary, agent-agnostic interface is
`sui prompt skill <bundle>` — that's how the surface is designed to work for any agent.
