# DeepBook Predict examples

Runnable TypeScript sources for the DeepBook Predict docs. The docs pull these
files in with `<ImportContent mode="code" tag="..." />`, so the published
samples stay tied to code that type-checks.

Standalone package (excluded from the root pnpm workspace because it pins
`@mysten/sui` 2.x). Build it on its own:

```sh
npm install
npm run build   # tsc --noEmit
```

Each source file wraps its doc chunk in `// docs::#<tag>` / `// docs::/#<tag>`
markers. The tags map to these pages:

- `config`, `client`, `create-manager`, `mint-binary`: DeepBook Predict landing quickstart.
- `config`, `client`, `oracle`, `mint-binary`, `mint-range`, `redeem`, `supply`, `withdraw`: Testnet workflow tutorial.

The IDs in `src/config.ts` are Testnet-only and pinned to the
`predict-testnet-4-16` branch. They change at Mainnet launch.
