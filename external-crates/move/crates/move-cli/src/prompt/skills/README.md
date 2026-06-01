# move-prompt skills

Three audit skill bundles, embedded into the `move` binary at build time and accessed via the
`move prompt skill` subcommand. **This is an agent-agnostic interface** — any AI agent that can
shell out can read these by calling `move prompt skill <bundle>`. No filesystem install needed.

## Bundles

- **`sui-move-security-review`** — the SM-* rule catalog (~25 rules across 13 categories). Each
  rule has an invariant, a disassembly detection signal, a severity, and an exploit sketch.
  Includes `auditing-bytecode.md` (per-rule disassembly signals + the report format that pairs
  assembly evidence with a human view).
- **`sui-audit-toolchain`** — how to obtain bytecode for on-chain targets and stand up the
  `sui` / `move decompile` tools cleanly. Confirm-`SUI_REF`-before-build gate; isolation rule
  forbidding local Sui checkouts; `suiup`-managed `sui` CLI provenance check.
- **`move-bytecode-comprehension`** — Move binary format essentials + the survival table
  (what info from source survives compilation to bytecode); how to read disassembly and
  decompiled output without being misled by decompiler artifacts.

## Access (any AI agent)

```sh
move prompt skills                                              # list bundles
move prompt skill sui-move-security-review                      # read SKILL.md
move prompt skill sui-move-security-review --list               # list reference files
move prompt skill sui-move-security-review --file access-control  # read a reference file
```

These three commands cover everything an agent needs to learn the audit workflow.

## Lineage

The SM-* catalog in `sui-move-security-review/` was derived from the constructive
`MystenLabs/skills` "how to write correct Sui Move" guidance. See
`sui-move-security-review/LINEAGE.md` for the pinned upstream ref and the refresh protocol.

## Editing notes

- This directory is the canonical home for these skills. Files here are embedded into the
  binary at compile time by `move-cli/build.rs`. To distribute changes, rebuild the binary.
- The build is deterministic — files are sorted by path before embedding (see
  `prompt_skills_snapshot` test).
- A model-agnostic-language sweep applies: no skill content references a specific AI model or
  vendor; use generic "the audit agent" wording.

## Appendix — optional Claude Code skill auto-discovery

For users running Claude Code who prefer native skill auto-discovery (instead of pulling each
file via `move prompt skill`), the bundles in this directory can also be installed via the
standard `skills` CLI:

```sh
npx skills add path/to/move-cli/skills --skill '*' --agent claude-code --global -y
```

This is purely optional convenience. The primary, agent-agnostic interface is the `move prompt
skill <bundle>` CLI access — that's how the audit workflow is designed to work for any agent.
