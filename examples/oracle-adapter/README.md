# Oracle adapter example

Reference material for the "Oracles for DeFi on Sui" docs cluster.

- `move/` — the `oracle_adapter` Move package: a provider-neutral price adapter
  over Pyth with staleness, confidence, deviation, and fallback guards, plus a
  `demo` module that emits a read price. Build and test on its own:

  ```sh
  cd move && sui move test
  ```

- `ts/` — Pyth TypeScript samples (pull update + read, the stale-read rejection,
  and the keeper push cycle) on `@mysten/sui` 2.x and `pyth-sui-js` 4.x.
  Standalone package:

  ```sh
  cd ts && npm install && npm run build   # tsc --noEmit
  ```

- `ts-switchboard/` — the Switchboard on-demand sample, in its own package
  pinned to `@mysten/sui` 1.x. `@switchboard-xyz/sui-sdk` has not migrated to
  the 2.x SDK, and the two `Transaction` types are not interchangeable, so this
  sample cannot share a package with the Pyth ones:

  ```sh
  cd ts-switchboard && npm install && npm run build   # tsc --noEmit
  ```

The docs pull chunks with `<ImportContent>`, so samples stay tied to code that
compiles. `ts/run.mts` is a local execution harness (not committed): it loads the
active sui keystore key in memory only and executes the Pyth flows on Testnet.
