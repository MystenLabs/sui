Sui indexer is an off-fullnode service to serve data from Sui protocol, including both data directly generated from chain and derivative data.

## Architecture
![enhanced_FN](https://user-images.githubusercontent.com/106119108/221022505-a1d873c6-60e2-45f1-b2aa-e50192c4dfbb.png)


## Steps to run locally
### Prerequisites
- install local [Postgres server](https://www.postgresql.org/download/). You can also `brew install postgres@version` and then add the following to your `~/.zshrc` or `~/.zprofile`, etc: 
```sh
export LDFLAGS="-L/opt/homebrew/opt/postgresql@15/lib"
export CPPFLAGS="-I/opt/homebrew/opt/postgresql@15/include"
export PATH="/opt/homebrew/opt/postgresql@15/bin:$PATH"
```
- make sure you have libpq installed: `brew install libpq`, and in your profile, add `export PATH="/opt/homebrew/opt/libpq/bin:$PATH"`. If this doesn't work, try `brew link --force libpq`.

- install Diesel CLI with `cargo install diesel_cli --no-default-features --features postgres`, refer to [Diesel Getting Started guide](https://diesel.rs/guides/getting-started) for more details
- [optional but handy] Postgres client like [Postico](https://eggerapps.at/postico2/), for local check, query execution etc.


### Local Development(Recommended)

Use [sui-test-validator](../../crates/sui-test-validator/README.md)


### Running standalone indexer
1. DB setup, under `sui/crates/sui-indexer` run:
```sh
# an example DATABASE_URL is "postgres://postgres:postgres@localhost/gegao"
diesel setup --database-url="<DATABASE_URL>"
diesel migration run --database-url="<DATABASE_URL>"
```
Note that you'll need an existing database for the above to work. Replace `gegao` with the name of the database created.

2. Checkout to your target branch

For example, if you want to be on the DevNet branch
```sh
git fetch upstream devnet && git reset --hard upstream/devnet
```
3. Start indexer binary, under `sui/crates/sui-indexer` run:
```sh
# Change the RPC_CLIENT_URL to http://0.0.0.0:9000 to run indexer against local validator & fullnode
cargo run --bin sui-indexer -- --db-url "<DATABASE_URL>" --rpc-client-url "https://fullnode.devnet.sui.io:443" --reset-db --fullnode-sync-worker
```
Note that `sui-indexer` can run as a `fullnode-sync-worker`, which pulls data from fullnode and writes data to DB; `sui-indexer` can also run as a RPC server with flag `--rpc-server-worker`, more flags info can be found in this [file](https://github.com/MystenLabs/sui/blob/main/crates/sui-indexer/src/lib.rs#L83-L123).
### DB reset
Run this command under `sui/crates/sui-indexer`, which will wipe DB; In case of schema changes in `.sql` files, this will also update corresponding `schema.rs` file.
```sh
diesel database reset --database-url="<DATABASE_URL>"
```

## Integration test
Integration tests in the `integration_tests.rs` will be run by GitHub action as part of the CI checks
to run the test locally, start a Postgresql DB and run the test using following command:
```sh
POSTGRES_PORT=5432 cargo test --package sui-indexer --test integration_tests --features pg_integration
```

If you run into an error like `DatabaseError(UniqueViolation, "duplicate key value violates unique constraint \"pg_proc_proname_args_nsp_index\""))'` you can try running each integration test individually with
```sh
POSTGRES_PORT=5432 cargo test --package sui-indexer --test integration_tests --features pg_integration -- --test-threads=1
```

And to execute a single test such as just `test_event_query_e2e`, you can do
```sh
POSTGRES_PORT=5432 cargo test test_event_query_e2e --package sui-indexer --test integration_tests --features pg_integration -- --test-threads=1
```

**Note** all existing data will be wiped during the test.
