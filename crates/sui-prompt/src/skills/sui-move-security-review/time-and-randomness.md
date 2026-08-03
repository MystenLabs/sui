# L — Time & on-chain randomness

Two distinct classes. Time-source misuse is a correctness bug; randomness misuse is a
high-severity, frequently-missed exploit class that the constructive skills do not cover.

### SM-L1 — Imprecise time source for deadlines   [High]
Invariant: deadline / auction / vesting / cooldown logic reads the `Clock` shared object (`0x6`,
`clock.timestamp_ms()`), not `ctx.epoch_timestamp_ms()` — the latter is the epoch *start* time
and is identical for every transaction within an epoch.
Detect: time-sensitive comparisons against `ctx.epoch_timestamp_ms()` / `ctx.epoch()` where
sub-epoch precision matters.
Exploit: a "deadline" that never advances within an epoch — submit late, or treat an expired
window as still open (or vice-versa).
Source: `MystenLabs/skills → sui-move/move.md`, `MystenLabs/skills → sui-overview/ecosystem.md`.

### SM-L2 — Randomness test-and-abort (composition leak)   [Critical]
Invariant: functions that consume `sui::random` (`Random` object `0x8`) must be `entry` only,
NOT `public` (the framework rejects randomness in `public` functions). Beyond that compile-time
rule, the consuming `entry` function must (a) return no value derived from the randomness and
(b) finalize the outcome's effect within the same call so the caller cannot observe the roll and
then abort.
Detect: `random::*` / `new_generator` in a `public` function (won't compile — relevant only when
reviewing incomplete/source code); an `entry` function that consumes randomness and returns,
emits, or branches on the result before performing the observable consequence (mint rare item,
pay out) such that a PTB could read it and abort.
Exploit: the attacker wraps the call in a PTB, inspects the outcome (return value, created
object, or a follow-on assertion), and **aborts the whole transaction** when the roll is
unfavorable — retrying until they win. Breaks lotteries, loot boxes, raffles, random rewards.
Why missed: the code "works" and tests pass; the flaw is purely in composability/visibility.
Source: `MystenLabs/skills → sui-overview/ecosystem.md` (entry-only / test-and-abort) + Sui randomness
best-practice for the value-reveal nuance.
