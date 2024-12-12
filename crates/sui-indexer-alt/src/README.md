# sui-indexer-alt

## Running
Need to provide a config file, and either remote-store-url or local-ingestion-path. If both are provided, remote-store-url will be used.

```
cargo run --bin sui-indexer-alt -- --database-url {url} indexer --remote-store-url https://checkpoints.mainnet.sui.io --skip-watermark --first-checkpoint 68918060 --last-checkpoint 68919060 --config indexer_alt_config.toml
```
