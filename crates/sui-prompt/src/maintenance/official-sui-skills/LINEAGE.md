# Lineage — `official-sui-skills` (pointer to upstream)

The `official-sui-skills` bundle is a **pointer**, not derived content. It
hardcodes the snapshot URL at the pin documented in `maintenance/UPSTREAMS.md`,
so the agent fetching from it lands on the same upstream snapshot the rest of
`sui prompt` tracks.

## What gets hardcoded at the pinned ref

Inside `skills/official-sui-skills/SKILL.md`:

- URLs containing the SHA (multiple forms: `/tree/<sha>/...`, `/blob/<sha>/...`,
  `raw.githubusercontent.com/.../<sha>/...`).
- The *High-level scope at the pinned ref* enumeration, which reflects the
  upstream directory layout at that snapshot.

## Refresh protocol

When the pin in `maintenance/UPSTREAMS.md` bumps:

1. Search-and-replace the old SHA with the new SHA in
   `skills/official-sui-skills/SKILL.md` (appears in multiple URLs).
2. Re-fetch the upstream `SKILL.md` at the new SHA. Compare its directory list
   to the *High-level scope at the pinned ref* section in
   `skills/official-sui-skills/SKILL.md`. Update that list if upstream skill
   directories were added, removed, or renamed.
3. Rebuild the Sui CLI so the embedded SKILL.md reflects the new ref.
