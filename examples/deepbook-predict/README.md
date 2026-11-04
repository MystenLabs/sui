# DeepBook Predict examples

TypeScript sources for the DeepBook Predict docs. The docs pull these files in
with `<ImportContent mode="code" tag="..." />`, so the published samples stay
tied to code that type-checks.

The examples build transactions and return them. They never sign, because the
SDK never signs and never holds keys: every `client.predict.tx.*` builder hands
back a `Transaction` for a wallet, dapp-kit, or your own signer to execute.

Standalone package, excluded from the root pnpm workspace. Install and build it
on its own:

```sh
npm install
npm run build   # tsc --noEmit
```

`npm run build` is the only thing that type-checks this package. No workflow in
`.github/workflows` covers `examples/`, so run it locally after any edit.

Each source file wraps its doc chunk in `// docs::#<tag>` / `// docs::/#<tag>`
markers. The tags map to these pages:

- `config`, `client`, `create-manager`, `markets`, `mint-binary`: DeepBook Predict landing quickstart.
- `config`, `client`, `create-manager`, `markets`, `queue-mint`, `queue-sell`, `supply`, `withdraw`: Testnet workflow tutorial.
- `queue-mint`, `queue-sell`: Delayed Execution contract reference and the Positions SDK page.
- `mint-binary`, `mint-range`, `redeem`: the immediate trade path, on the Predict contract reference.
- `sessions-authorize`, `sessions-list`, `sessions-trade`, `sessions-queue`, `sessions-revoke`: Sessions contract reference.

From Predict protocol version 4, a network's trades are queued orders: an
order is placed now and filled or refunded shortly after, at Pyth's signed
price for a fixed instant. `queue-mint.ts`, `queue-sell.ts` and
`sessions-queue.ts` use that path. `mint.ts`, `mint-range.ts`, `redeem.ts` and
the trade wrappers in `sessions-trade.ts` are the immediate path, which a
network runs only before its delayed-execution cutover. Both networks have
passed it, so those files now abort on both, and only Predict protocol version
4 and Sessions protocol version 3 accept calls.
`client.predict.read.executionMode()` reports which path a network is on. Mainnet needs `@mysten/deepbook-v3` 2.8.0 or later, the floor in
`package.json`, because earlier releases call Predict and Sessions versions that
the Mainnet cutover retired.

The `sessions-*` files import from `@mysten/deepbook-v3/sessions`, a separate
subpath from `/predict`. They build owner-signed grant transactions and
session-signed trades, and like everything else here they only return the
transaction.

No package ID, object ID, or coin type is hardcoded here. They all come from
`@mysten/deepbook-v3/predict`, which carries a record for each deployment its
release was cut against. `src/config.ts` asserts that record is
`deepbook-predict-testnet` on Testnet and `deepbook-predict-mainnet` on Mainnet
at startup, so an SDK upgrade that moves a network to a new deployment fails
immediately instead of trading against untested IDs.

The examples default to Testnet, where the quote coin is a mintable test coin
that displays as DUSDC. Set `NETWORK` to `mainnet` in `src/config.ts` to target
the Mainnet deployment, which settles in native USDC. Read the quote coin from
`getConfig(network).quoteCoinType` rather than assuming either.
