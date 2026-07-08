# Docs Audit Recommendations

Generated from `pnpm audit:summary` on 2026-07-08.

**Current state after fixes:** 639 pages, 393 with issues, 338 with goals (294 passing, 44 failing).

---

## Concept Coverage Gaps

These pages are expected to cover specific concepts but are missing key terms.

### 1. `frontend-wallet` gap in `app-frontends.mdx`

**Problem:** `getting-started/onboarding/app-frontends.mdx` only has 2/5 wallet-related terms. Missing: `ConnectButton`, `useCurrentAccount`, `@mysten/dapp-kit`.

**Recommendation:** The page currently uses raw SDK calls instead of dApp Kit hooks. Add a section or callout showing the dApp Kit equivalent — `ConnectButton` for wallet connection, `useCurrentAccount` for reading the connected address, and the `@mysten/dapp-kit` import. This aligns the page with the dApp Kit frontend example and creates a clearer on-ramp to `dapp-kit-frontend.mdx`.

### 2. `ptb` gap in migration guides

**Problem:** `sui-for-ethereum.mdx` and `sui-for-solana.mdx` mention PTBs conceptually ("programmable transaction") but don't show SDK methods like `splitCoins`, `moveCall`, or `transferObjects`.

**Recommendation:** Add a short code snippet in the PTB section of each migration guide showing a basic PTB in TypeScript:
```ts
const tx = new Transaction();
const [coin] = tx.splitCoins(tx.gas, [1000]);
tx.transferObjects([coin], recipient);
```
This gives migrating developers something concrete instead of just prose. Link to `/develop/transactions/ptbs/building-ptb` for the full guide.

### 3. `capability-pattern` gap in `dev-cheat-sheet.mdx`

**Problem:** The cheat sheet only has 1/5 capability pattern terms. Missing: `Cap`, `AdminCap`, `transfer`, `access control`.

**Recommendation:** Add a "Capability pattern" bullet under the Move section:
```
- Use capability objects (`AdminCap`, `MintCap`) for access control instead of address checks. Transfer capabilities to delegate, destroy to revoke. See [Capability Pattern](/getting-started/examples/capability-pattern).
```

---

## Stub/Thin Pages (need content)

Pages with < 100 words that are effectively empty. These should either be filled out or redirected.

| Page | Words | Recommendation |
|------|-------|----------------|
| `develop/publish-upgrade-packages/deploy.mdx` | 0 | **Critical.** Empty page. Write deployment guide or redirect to `upgrade.mdx`. |
| `develop/testing-debugging/testing.mdx` | 18 | **Critical.** Near-empty. This is a primary landing page for testing — needs Move test guide content. |
| `onchain-finance/fungible-tokens/integrating-with-stablecoins.mdx` | 35 | Stub. Add stablecoin integration patterns (USDC on Sui, bridged assets). |
| `develop/objects/display/display-preview.mdx` | 30 | Stub. Add Object Display preview examples. |
| `references/contribute/localize-sui-docs.mdx` | 36 | Stub. Either flesh out localization guide or remove if not supported. |
| `references/fullnode-protocol.mdx` | 36 | Thin landing page. Add overview text or merge with `fullnode-protocol-messages.mdx`. |
| `references/fullnode-protocol-messages.mdx` | 33 | Same — these three gRPC reference pages are near-empty wrappers. |
| `references/fullnode-protocol-types.mdx` | 33 | Same. |
| `references/sui-framework-reference.mdx` | 46 | Thin redirect page. Fine as-is if it links to the generated framework docs. |
| `references/package-managers/manifest-reference.mdx` | 65 | Has a TODO about finding a better example package. Flesh out with complete `Move.toml` reference. |
| `references/rust-sdk.mdx` | 71 | Thin pointer page. Add quickstart code or merge into `sdk-comparison.mdx`. |

## Pages Slightly Under Threshold (150-300 words)

These are close to passing and may just need a paragraph or two:

| Page | Words | Issue |
|------|-------|-------|
| `develop/cryptography/hashing.mdx` | 192 | Add usage examples for each hash function |
| `develop/write-move/move-fundamentals.mdx` | 97 | Add content or redirect — this may be a pointer page |
| `develop/objects/object-ownership/shared.mdx` | 154 | Add practical guidance on when to use shared objects |
| `develop/objects/object-ownership/index.mdx` | 255 | Close to threshold — add a summary paragraph |
| `develop/objects/display/display-overview.mdx` | 265 | Close — add an example |
| `onchain-finance/closed-loop-token/spending.mdx` | 265 | Close — expand the spending flow explanation |
| `onchain-finance/payments.mdx` | 225 | Also missing `keywords`. Expand payments overview. |
| `operators/data-management/archives.mdx` | 179 | Add operational guidance on enabling archives |
| `operators/genesis.mdx` | 150 | Add genesis ceremony steps or context |
| `operators/full-node/monitoring.mdx` | 106 | Add Prometheus/Grafana setup instructions |

## Example Pattern Pages (wrong archetype)

The 7 pages in `onchain-finance/examples-patterns/` were classified as "example" pages but don't follow the bootcamp template (missing When to use, Prerequisites, Setup, Run, Troubleshooting, Key code sections). They're actually **code snippet galleries** — short Move examples with explanations.

**Recommendation:** Either:
- **(A)** Rewrite them to follow the bootcamp example template with full setup/run instructions, or
- **(B)** Reclassify them as "guide" pages in the goal generator so the checks match their actual format. This is the faster fix.

Affected pages:
- `fixed-supply.mdx` (94 words)
- `in-game-currency.mdx` (284 words)
- `kiosk.mdx` (409 words)
- `loyalty-tokens.mdx` (321 words)
- `nft-rental.mdx` (534 words)
- `soulbound-tokens.mdx` (376 words)
- `wasm-template.mdx` (585 words)

## Onboarding Path Gaps

| Page | Issue | Fix |
|------|-------|-----|
| `get-coins.mdx` | No link to next step (`/getting-started/onboarding/hello-world`) | Add a "Next steps" link at the bottom |
| `sui-install.mdx` | Code is inside `ImportContent`, not inline code fences | Content issue — goal already adjusted to match `ImportContent` pattern |
| `install-binaries.mdx` | No `sui --version` verification step shown | Add a "Verify installation" section |
| `install-source.mdx` | No `sui --version` verification step shown | Add a "Verify installation" section |

## Duplicate Titles (informational)

These are titles shared across different sections. They don't cause build issues but can confuse search results:

- "Design" (3 pages across DeepBook, Margin, Predict)
- "Contract Information" (3 pages)
- "Orders" / "Orders SDK" (2 each)
- "Security" / "Testing" (2-3 each)

**Recommendation:** Prefix with the product name (e.g., "DeepBook Design", "DeepBook Margin Design") for better searchability.
