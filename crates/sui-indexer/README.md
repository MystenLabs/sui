Sui indexer is an off-fullnode service to serve data from Sui protocol, including both data directly generated from chain and derivative data.

## Current architecture Dec 2022 (will change soon)
![indexer_simple](https://user-images.githubusercontent.com/106119108/209000367-4c7d23d8-fef2-4485-8472-89c31f0e2d62.png)

## Steps to run locally
### Prerequisites
- install local [Postgres server](https://www.postgresql.org/download/)
- install Diesel CLI, you can follow the [Diesel Getting Started guide](https://diesel.rs/guides/getting-started) up to the *Write Rust* section
- [optional but handy] Postgres client like [Postico](https://eggerapps.at/postico2/), for local check, query execution etc.

### Steps
1. DB setup
```sh
# DB setup, run the following commands from the /sui-indexer folder
# .env file under /sui-indexer is required for diesel cmds
# in .env file, DATABASE_URL should point to your local PG server
# an example is:
# DATABASE_URL="postgres://postgres:postgres@localhost/gegao"
diesel setup

# and then run 
diesel migration run
```
2. checkout the latest devnet commit by running commands below, otherwise API version mismatch could cause errors
```sh
git fetch upstream devnet
git reset --hard upstream/devnet
```
3. Go to `sui/crates/sui-indexer` and run the following command:
```sh
# DATABASE_URL should be the same value as above
cargo run --bin sui-indexer -- --db-url "<DATABASE_URL>" --rpc-client-url "https://fullnode.devnet.sui.io:443"
```
  
### Clean up and re-run
- Run `diesel migration revert` under `/sui-indexer` until no more tables are deleted;
- Also delete `__diesel_schema_migrations`, you can do this via Postico client
