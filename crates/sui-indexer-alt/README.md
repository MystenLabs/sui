# sui-indexer-alt

## Running
A source of checkpoints is required (exactly one of `--remote-store-url` or
`--local-ingestion-path`), and a `--config` much be supplied.

```
cargo run --bin sui-indexer-alt -- indexer               \
  --database-url {url}                                   \
  --remote-store-url https://checkpoints.mainnet.sui.io  \
  --skip-watermark                                       \
  --first-checkpoint 68918060 --last-checkpoint 68919060 \
  --config indexer_alt_config.toml
```

## Pruning

Some pipelines identify regions to prune by transaction sequence number, or by
epoch. These pipelines require the `cp_sequence_numbers` table be populated in
the database they are writing to, otherwise they are unable to translate a
checkpoint sequence range into a transaction or epoch range.

Only one instance of the indexer writing to that database needs to populate
this table, by enabling the `cp_sequence_numbers` pipeline.
