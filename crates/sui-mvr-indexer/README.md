The MVR indexer is a spin-off of the Sui indexer. It has a subset of the full indexer schema, limited to just the tables needed to support MVR. The required tables are `epochs`, `checkpoints`, `packages`, `objects_snapshot`, and `objects_history`. This enables the custom indexer to support the `package_by_name` and `type_by_name` queries on GraphQL.

# Running this indexer
## Start the Postgres Service

Postgres must run as a service in the background for other tools to communicate with.  If it was installed using homebrew, it can be started as a service with:

``` sh
brew services start postgresql@version
```

## DB reset
When making db-related changes, you may find yourself having to run migrations and reset dbs often. The commands below are how you can invoke these actions.
```sh
cargo run --bin sui-mvr-indexer -- --database-url "<DATABASE_URL>" reset-database --force
```

## Start the indexer
```SH
cargo run --bin sui-mvr-indexer -- --db-url "<DATABASE_URL>" indexer --rpc-client-url "https://fullnode.devnet.sui.io:443" --remote-store-url  http://lax-suifn-t99eb.devnet.sui.io:9000/rest
```

## Migrations

To add a new table, run `diesel migration generate your_table_name`, and modify the newly created `up.sql` and `down.sql` files.

You would apply the migration with `diesel migration run`, and run the script in `./scripts/generate_indexer_schema.sh` to update the `schema.rs` file.
