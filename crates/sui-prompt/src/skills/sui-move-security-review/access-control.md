# A — Access control & capabilities (+ cap custody)

Capabilities are Sui's primary authorization primitive: holding the object *is* the permission.
Most high-severity findings live here. Reason about (1) the cap's ability set, (2) whether the
privileged path actually requires it, (3) whether the cap is bound to the resource it governs,
and (4) who can construct or obtain it.

### SM-A1 — Capability ability hygiene   [Critical]
Invariant: `*Cap` / authority structs must NOT have `copy` (duplicable authority) or `drop`
(silently destroyable → lockout); `store` only when transfer is genuinely intended.
Detect: `struct\s+\w*Cap\b[^{]*has[^{]*\b(copy|drop)\b`; also any struct used as authority that
carries `copy`/`drop`. The authority's **name doesn't matter** — it may not contain "Cap"
(e.g., `Admin`, `Manager`, `License`, or any project-specific name)
Exploit: clone an `AdminCap`/`TreasuryCap` and replay privileged calls; or grief by dropping the
only cap and bricking administration.
Source: `MystenLabs/skills → sui-move/move.md`, `MystenLabs/skills → naming-conventions/SKILL.md`.

### SM-A2 — Missing authorization on privileged entrypoint   [Critical]
Invariant: **every `public`/`entry` function that takes a `&mut SharedObject` is callable by any
address** — verify each one either (a) takes a capability parameter, (b) asserts on
`ctx.sender()` against a stored address/allow-list, or (c) is **intentionally permissionless and
the code/comments label it as such**. Read-only getters are exempt.
Detect: `public`/`entry` fns that mutate shared/global state with no `&\w*Cap` parameter and no
`sender()`/address assertion. Pay particular attention to functions that mint, withdraw, set
fees/config, pause, change ownership, or modify a deny list. Concretely: a function header with
a `&mut <SharedT>` arg lacking either a `&<Name>Cap` arg in the signature or an
`if (tx_context::sender(ctx) == <addr>) abort <code>` / `assert!(tx_context::sender(ctx) == <addr>, ...)`
gate before the first state-mutating expression.
_Absence rule:_ walk every `public`/`entry` fn touching shared state; a `&*Cap` or
`tx_context::sender(...)` gate *elsewhere* in the module does not clear an unguarded fn.
Exploit: any address calls the privileged path directly via a PTB.
Source: `MystenLabs/skills → sui-move/move.md`, `MystenLabs/skills → composable-move-functions/SKILL.md`.

### SM-A3 — Capability not bound to the resource it governs   [Critical] [+domain]
Invariant: when a cap governs a *specific* object (a pool, vault, treasury), the cap stores that
object's `ID` and the privileged fn asserts `cap.<id_field> == object::id(target)`. A bare
`&AdminCap` that gates actions on *any* instance of a type is a confused-deputy bug.
Detect: cap-gated fn taking `&SomeCap` + a target object, where `SomeCap` has no `ID`/`address`
field, or has one but it is never compared to the target.
_Absence rule:_ walk every fn taking `&*Cap` + target object; a
`cap.<id> == object::id(target)` comparison *elsewhere* does not clear an unchecked fn.
Example:
```move
// Vulnerable: AdminCap has no pool binding; every Pool accepts it.
public fun withdraw(cap: &AdminCap, pool: &mut Pool, amount: u64, ctx: &mut TxContext) {
    let coins = balance::split(&mut pool.reserves, amount);
    transfer::public_transfer(coin::from_balance(coins, ctx), tx_context::sender(ctx));
}

// Patched: PoolCap carries the pool_id it governs.
public fun withdraw(cap: &PoolCap, pool: &mut Pool, amount: u64, ctx: &mut TxContext) {
    assert!(cap.pool_id == object::id(pool), EWrongPool);
    let coins = balance::split(&mut pool.reserves, amount);
    transfer::public_transfer(coin::from_balance(coins, ctx), tx_context::sender(ctx));
}
```
*This is one shape, not the only shape.* SM-A3 fires on any fn taking an
authority-bearing struct + the resource it governs without asserting their
binding. The authority's **name doesn't matter** — it may not contain "Cap"
(e.g., `Admin`, `Manager`, `License`, or any project-specific name). Other
variants: id stored in a differently-named field, cap wrapped in a shared
object, one cap governing several resources. The absence rule above is
authoritative; identify authorities by *role*, not by name.
Exploit: obtain (or legitimately own) one cap for your own object, then pass someone else's
object to the same function — operate on / drain a resource you don't control.
Source: [+domain] (classic cross-pool / cross-vault authority bug).

### SM-A4 — One-step authority handoff   [High]
Invariant: transferring an authority cap uses a two-step propose→accept flow where `accept`
verifies `ctx.sender() == proposed_new_admin`.
Detect: direct `transfer::transfer(cap, addr)` / `public_transfer(cap, addr)` of an authority cap
to an externally-supplied address with no acceptance step.
Exploit: a mistyped or attacker-influenced address permanently captures or bricks admin authority
(unrecoverable, since the cap is the only key).
Source: `MystenLabs/skills → sui-move/move.md`.

### SM-A5 — Forgeable witness / authority type   [High]
Invariant: a type used as proof of authority (witness, OTW) must be constructible ONLY by its
owning module — i.e. module-private fields, or a genuine One-Time-Witness. A `public struct W has
drop {}` with no fields can be built by anyone.
Detect: functions gated by a `W: drop` witness parameter where `W` is publicly constructible
(public struct with public/no fields, or a generic type the caller chooses).
Exploit: construct the witness yourself and call the "authorized-only" function.
Source: `MystenLabs/skills → sui-move/move.md` (witness / OTW patterns).

### SM-A6 — Missing object-state guard on a privileged release/mutate   [High]
Invariant: when a module exposes a privileged release / transfer / state-change function for an
object it controls — e.g. a custom transfer (object lacks `store`), a release-from-escrow,
unlock-after-deadline, redeem-receipt, finalize-vote, or close-position pattern — it MUST assert
the relevant **object-state invariant** (`unlocked`, `expired_at < now`, `paid >= due`,
`!revoked`, `finalized`) before performing the privileged operation. This is distinct from SM-A2
(caller-identity gate) and SM-D1 (caller-supplied bounds): it is a *self-state* guard on the
object being acted on. Forgetting it means the gate exists in the type system but not in the
runtime check.
Detect: the privileged operation (`transfer::transfer<T>(...)`, `balance::join(...)`,
`coin::take(...)`, `balance::split(...)`, internal state mutator) is reached on a path
where the controlling field (`obj.unlocked`, `obj.expiry`, `obj.paid_back`, …) is read
but never compared+gated, OR not read at all. Concretely: the privileged call site's
preceding code contains no read of `self.<state>` / `&self.<state>` / `&mut self.<state>`
followed by an `==` / `<` / `>` comparison and an `if (...) abort <code>` / `assert!(...)`
guard. (Cross-check: if the type has no controlling field at all, the privileged path
is unconditional — the high-severity shape.)
_Absence rule:_ an `assert!`/`abort` *elsewhere* in the module does not clear the rule —
verify the guard reaches *this* privileged op on the path.
Exploit: caller invokes the release path while the object's state still forbids it — transfer a
locked NFT, redeem before expiry, withdraw without the loan being marked repaid.
Source: `MystenLabs/skills → object-model/transfers.md` (`transfer_if_unlocked` example:
`assert!(item.unlocked, EItemLocked); transfer::transfer(item, to);`).

---

## Capability custody (cross-ref to coins in `arithmetic-and-coins.md`)

### SM-G1-custody — Privileged caps reachable or mis-routed   [Critical]
Invariant: `TreasuryCap`, `DenyCap`, `UpgradeCap` and bespoke admin caps must be transferred to a
trusted holder (publisher/multisig) or locked in a shared object behind explicit checks — never
`public_transfer`'d to an arbitrary/caller-supplied address, and never reachable by an
unauthenticated function.
Detect: caps created in `init` or factory fns then sent to a non-fixed address; mint/burn/upgrade
operations whose cap argument is obtainable without authorization.
Exploit: seize mint/upgrade authority → unlimited supply or full package rewrite.
Source: `MystenLabs/skills → sui-move/events-coins.md`, `MystenLabs/skills → sui-publish/SKILL.md`. See SM-G1, SM-I1.
