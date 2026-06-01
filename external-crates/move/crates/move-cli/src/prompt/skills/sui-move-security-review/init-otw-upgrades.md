# H/I — Init, One-Time-Witness & package upgrades

`init` runs once at publish and is unrecoverable; OTW is the proof-of-publish primitive; the
`UpgradeCap` is authority over all future code. Upgrades preserve struct types under the original
package ID but do NOT re-run `init` — versioning gaps are a top finding.

### SM-H1 — Malformed One-Time-Witness   [High]
Invariant: an OTW struct is named exactly `MODULE_NAME` in ALL CAPS, has only `drop`, has no
fields, and is consumed once in `init`. Framework calls (e.g. `coin::create_currency`) rely on it
as proof of module authority.
Detect: a struct passed to `create_currency`/other OTW consumers that has extra abilities, has
fields, or whose name doesn't match the module; OTW-typed values that can be obtained outside
`init`.
Exploit: a malformed/forgeable witness undermines the "happens once at publish" guarantee the
caller relies on for authority (ties to SM-A5).
Source: `MystenLabs/skills → sui-move/move.md`.

### SM-H2 — Unsafe `init` capability routing   [Critical]
Invariant: `init` routes every authority cap to the publisher (`ctx.sender()`) or a multisig, or
locks it behind gating — never drops it, sends it to `@0x0` / a hardcoded foreign address, or
shares it without checks.
Detect: caps created in `init` then `transfer`'d to a constant/derived address, dropped, or
`share_object`'d with an ungated mutator.
Exploit: anyone claims admin/treasury at genesis, or authority is permanently lost — both
unrecoverable without redeploy.
Source: `MystenLabs/skills → sui-move/move.md`.

### SM-I1 — `UpgradeCap` custody / policy   [Critical]
Invariant: for any non-trivial package the `UpgradeCap` is held by a multisig, policy-restricted
(`only_additive_upgrades` / `only_dep_upgrades`), or burned (`package::make_immutable`) — not
left live in the single publishing EOA under the default Compatible policy.
Detect: publish flow that retains the `UpgradeCap` in the deployer key with no restriction; no
`make_immutable`/policy call; UpgradeCap discoverable as owned by an EOA. **Also (for governance /
upgrade-wrapper modules):** verify any module that mediates upgrades commits the `UpgradeReceipt`
via `Call package::commit_upgrade(...)` in the same flow as the `Upgrade` command — a wrapper that
authorizes/returns an `UpgradeReceipt` but never commits it leaves the upgrade un-finalized (and
worse, the receipt is a hot potato, so the tx would abort — but a wrapper that *stores* the
receipt or routes it improperly is a misuse worth flagging).
Exploit: whoever holds it (or compromises that key) silently rewrites package logic — total
compromise of every object/flow the package controls. Mishandled `UpgradeReceipt` in a wrapper
either bricks the upgrade flow (DoS) or, if the wrapper relaxes the receipt's ability set,
re-enters SM-J1 territory.
Source: `MystenLabs/skills → sui-publish/SKILL.md`, `MystenLabs/skills → sui-move/move.md`,
`MystenLabs/skills → ptbs/commands.md` (UpgradeReceipt + `package::commit_upgrade`).

### SM-I2 — Versioning / migration gap across upgrades   [Critical]
Invariant: shared/long-lived objects carry a version field that entrypoints assert
(`assert!(obj.version == CURRENT_VERSION)`); upgrades ship an explicit, gated migrator that bumps
it. Because `init` does NOT re-run on upgrade, any new singleton/state needs its own gated
initializer.
Detect: shared-object entrypoints with no version assertion; an upgraded package that adds
state/singletons with no migration entrypoint; struct-type queries that must use the *original*
package ID but use the upgraded one.
Exploit: the old package version (still callable) keeps mutating new-format state, or
new-vs-old logic disagree on layout/invariants → inconsistency, stuck funds, or drain. Forgotten
new-singleton init can be front-run and claimed.
Source: `MystenLabs/skills → sui-publish/SKILL.md`, `MystenLabs/skills → ptbs/commands.md` (upgrade does not call `init`).
