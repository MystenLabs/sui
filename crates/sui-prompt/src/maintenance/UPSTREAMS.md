# Pinned upstream references

Canonical pin facts for upstream repositories that `sui prompt` content
references or derives from.

## MystenLabs/skills

- Repository: <https://github.com/MystenLabs/skills>
- Pinned ref: `764f21a95e709f46c60877a59d6ee6f27d9ed91e`
- Title of HEAD commit at this ref: *"Merge pull request #19 from MystenLabs/fix/skill-gaps-from-dapp-builds"*
- Dependents: `sui-move-security-review/LINEAGE.md`, `official-sui-skills/LINEAGE.md`

## Refresh

A refresh against new upstream is a coordinated edit:

1. Update the **Pinned ref** (and commit title) above to the new SHA.
2. Follow each dependent's `LINEAGE.md` *Refresh protocol* — each describes
   what to update inside its own skill content given the new pin.

No dependent should silently advance while another stays on the old SHA.
