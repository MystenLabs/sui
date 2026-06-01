# F/G — Arithmetic & coins

Move aborts on native add/sub/mul overflow, so the dangerous arithmetic is the *silent* kind
(`as` truncation, rounding) and the *economic* kind (zero amounts, fee rounding). Coin/supply
bugs are almost always custody or gating bugs.

### SM-F1 — Silent integer truncation via `as`   [High]
Invariant: no `as` casts between integer widths; use `try_into` / `try_from` (or explicit
checked conversion) so out-of-range values abort instead of wrapping.
Detect: `\bas\s+u(8|16|32|64|128|256)\b`, especially on amounts, prices, indices, or supply.
Exploit: a large value truncates to a small one — mispriced trade, undercounted debt, or a cap
check passed with a wrapped value.
Source: `MystenLabs/skills → modern-move-syntax/SKILL.md`, `MystenLabs/skills → sui-move/SKILL.md`.

### SM-F2 — Rounding direction & zero/empty amounts   [High]
Invariant: integer division rounds *against* the user (protocol never loses); multiply before
divide to preserve precision; reject zero/empty amounts (`assert!(amount > 0)`), and empty
`SplitCoins`/vector inputs.
Detect: `a / b * c` ordering (precision loss); fee/share math with no rounding rationale; mint /
deposit / swap entrypoints lacking a non-zero amount check.
Exploit: round-to-zero to extract fees or mint value for free; zero-amount calls that create
something-for-nothing or divide-by-zero abort as griefing.
Source: [+domain]; `MystenLabs/skills → ptbs/commands.md` (empty `SplitCoins` fails pre-execution).

### SM-G1 — Mint/burn & deny-cap custody / gating   [Critical]
Invariant: `TreasuryCap` and `DenyCap` are held by a trusted party or locked behind explicit
checks; `coin::mint` / `coin::burn` are only reachable through an authorized path.
Detect: `coin::mint` / `coin::burn` reachable from a `public`/`entry` fn whose `TreasuryCap`
argument is obtainable without authorization; caps `public_transfer`'d to a caller-supplied
address; a shared object wrapping a `TreasuryCap` with an ungated mint fn.
Exploit: unlimited inflation (mint to self) or supply seizure → token value destroyed.
Source: `MystenLabs/skills → sui-move/events-coins.md`, `MystenLabs/skills → sui-move/move.md`. See SM-G1-custody.

### SM-G2 — Deny list defined but not enforced   [High]
Invariant: regulated coins use `coin::create_regulated_currency` and the system `DenyList`
(`0x403`); any *custom* deny/allow list (a `Table<address,bool>` / `VecSet<address>`) is actually
checked in every entrypoint it should gate.
Detect: a deny/allow structure that is written to (add/remove) but never read in transfer/action
paths; regulated-token logic that bypasses the deny check.
Exploit: a denied/sanctioned address transacts anyway — compliance bypass / unauthorized action.
Source: `MystenLabs/skills → sui-move/move.md`.
