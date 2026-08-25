# DeepBook spot examples

Runnable TypeScript sources for the DeepBook spot docs (fees, funding, and — in
later work — the hands-on order workflow). The docs pull each chunk in with
`<ImportContent mode="code" tag="..." />`, so published samples stay tied to code
that type-checks.

Standalone package (excluded from the root pnpm workspace because it pins
`@mysten/sui` 2.x). Build it on its own:

```sh
npm install
npm run build   # tsc --noEmit
```

Each `src/*.ts` file wraps its doc chunk in `// docs::#<tag>` / `// docs::/#<tag>`
markers: `client`, `fund-deep`, `read-fees`, `deep-required`, `bootstrap-swap`,
`create-manager`.

`run.mts` is a local execution harness, not a doc sample and not meant for
commit. It reads a Testnet key from `DEEPBOOK_DOCS_TESTNET_KEY` (env only, never
logged) and prints digests for maintainer verification:

```sh
export DEEPBOOK_DOCS_TESTNET_KEY='suiprivkey1...'   # same shell
npm install && npx tsx run.mts
```
