# sui-graphql-rpc

## Spinning up locally

### Setting up local db

Rpc 1.5 backs the graphql schema with a db based on IndexerV2 schema. To spin up a local db, follow the instructions at [sui-indexer](../sui-indexer/README.md) until "Running standalone indexer".

If you have not created a db yet, you can do so as follows:
```sh
psql -U postgres
CREATE DATABASE sui_indexer_v2;
```

You should be able to refer to the db url now:
`psql postgres://postgres:postgrespw@localhost:5432/sui_indexer_v2`

With the new db, run the following commands (also under `sui/crates/sui-indexer`):

```sh
diesel setup --database-url="<DATABASE_URL>" --migration-dir=migrations_v2
diesel migration run --database-url="<DATABASE_URL>" --migration-dir=migrations_v2
```

### Launching the server
See [src/commands.rs](src/commands.rs) for all CLI options.

```
cargo run --bin sui-graphql-rpc start-server [--rpc-url] [--db-url] [--port] [--host] [--config]
```

This will build sui-graphql-rpc and start an IDE:

```
Starting server...
Launch GraphiQL IDE at: http://127.0.0.1:8000
```

### Launching the server w/ indexer
For local dev, it might be useful to spin up an indexer as well. Instructions are at [Running standalone indexer](../sui-indexer/README.md#running-standalone-indexer).
