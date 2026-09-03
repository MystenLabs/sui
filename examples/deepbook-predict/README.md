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
- `config`, `client`, `create-manager`, `markets`, `mint-binary`, `mint-range`, `redeem`, `supply`, `withdraw`: Testnet workflow tutorial.
- `sessions-authorize`, `sessions-list`, `sessions-trade`, `sessions-revoke`: Sessions contract reference.

The `sessions-*` files import from `@mysten/deepbook-v3/sessions`, a separate
subpath from `/predict`. They build owner-signed grant transactions and
session-signed trades, and like everything else here they only return the
transaction.

No package ID, object ID, or coin type is hardcoded here. They all come from
`@mysten/deepbook-v3/predict`, which carries the record for the deployment its
release was cut against. `src/config.ts` asserts that record is
`predict-testnet-8-21` at startup, so an SDK upgrade that moves Testnet to a new
deployment fails immediately instead of trading against untested IDs.

DeepBook Predict is deployed on Testnet only. There is no Mainnet deployment, and
`getConfig('mainnet')` throws.
