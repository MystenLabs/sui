# SuiNS Indexer

This indexer is used to cache the on-chain state of the SuiNS registry to a database,
in order to unlock more composite queries (e.g. query all subnames for a given name).

## Setting up locally

Copy `.env.sample` to `.env` and fill the variables (for DB connection). 
This sample env setup will work with mainnet types.

- `BACKFILL_PROGRESS_FILE_PATH`: It is expected (a file) in the format `{ "suins_indexing": <starting_checkpoint> }`.
- `CHECKPOINTS_DIR`: Just make sure an empty directory exists on that path.
