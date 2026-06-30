# sui prompt skills

Canonical location for the skill bundles embedded into `sui` at build time.
Each bundle is a directory holding a `SKILL.md` (routing / summary) plus
one or more reference files (`<topic>.md`) with the actual content;
categories (see `../categories/`) organize bundles by use case.

A skill bundle can belong to more than one category — categories reference skill bundles
by name, and skill bundles live here in one canonical location.

## Provenance

Pinned upstream refs, derivation methodology, and refresh protocols live in
`../maintenance/` — kept out of the embedded surface so the agent's context
doesn't carry provenance metadata.

## Editing notes

- A skill's content is agent-model-agnostic — no references to a specific AI
  model or vendor; generic "the agent" wording is the convention.
