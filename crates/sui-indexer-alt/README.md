# sui-indexer-alt

## Set-up

Running the indexer requires a Postgres-compatible database to be installed and running on the
system.

### Postgres

#### Postgres 15 setup
1. Install postgres
   ```sh
   brew install postgresql@15
   ```
   Resulting in this connection string
   ```
   postgresql://$(whoami):postgres@localhost:5432/postgres
   ```

### AlloyDB Omni

#### Docker setup
1. Install docker
   ```sh
   brew install --cask docker
   ```
2. Run docker (requires password)
   ```sh
   open -a Docker
   ```
#### AlloyDB setup
1. Run AlloyDB Omni in docker
   ```sh
   docker run --detach --publish 5433:5432 --env POSTGRES_PASSWORD=postgres_pw google/alloydbomni
   ```
   Resulting in this connection string
   ```
   postgresql://postgres:postgres_pw@localhost:5433/postgres
   ```

The indexer will try to connect to the following database URL by default:

```
postgres://postgres:postgrespw@localhost:5432/sui_indexer_alt
```

This database can be created with the following commands (run from this directory):

```sh
# Install the CLI (if not already installed)
cargo install diesel_cli --no-default-features --features postgres

# Use it to create the database and run migrations on it.
diesel setup                                                                       \
    --database-url="postgres://postgres:postgrespw@localhost:5432/sui_indexer_alt" \
    --migration-dir ../sui-indexer-alt-schema/migrations
```

If more migrations are added after the database is set-up, the indexer will
automatically apply them when it starts up.

## Tests

Tests require postgres to be installed (but not necessarily running), and
benefit from the following tools:

```sh
cargo install cargo-insta   # snapshot testing utility
cargo install cargo-nextest # better test runner
```

The following tests are related to the indexer (**run from the root of the
repo**):

```sh
cargo nextest run                \
    -p sui-indexer-alt           \
    -p sui-indexer-alt-framework \
    -p sui-indexer-alt-e2e-tests
```

The first package is the indexer's own unit tests, the second is the indexing
framework's unit tests, and the third is an end-to-end test suite that includes
the indexer as well as the RPCs that read from its database.

## Configuration

The indexer is mostly configured through a TOML file, a copy of the default
config can be generated using the following command:

```sh
cargo run --bin sui-indexer-alt -- generate-config > indexer_alt_config.toml
```

## Running
A source of checkpoints is required (exactly one of `--remote-store-url`,
`--local-ingestion-path`, or `--rpc-api-url`), and a `--config` must be
supplied (see "Configuration" above for details on generating a configuration
file).

```sh
cargo run --bin sui-indexer-alt -- indexer               \
  --database-url {url}                                   \
  --remote-store-url https://checkpoints.mainnet.sui.io  \
  --config indexer_alt_config.toml
```

## Pruning

Some pipelines identify regions to prune by transaction sequence number, or by
epoch. These pipelines require the `cp_sequence_numbers` table be populated in
the database they are writing to, otherwise they are unable to translate a
checkpoint sequence range into a transaction or epoch range.

Only one instance of the indexer writing to that database needs to populate
this table, by enabling the `cp_sequence_numbers` pipeline.
