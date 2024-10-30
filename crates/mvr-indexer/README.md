Sui indexer is an off-fullnode service to serve data from Sui protocol, including both data directly generated from chain and derivative data.

&#9888; **Warning:** Sui indexer is still experimental and we expect occasional breaking changes that require backfills.

## Architecture
![enhanced_FN](https://user-images.githubusercontent.com/106119108/221022505-a1d873c6-60e2-45f1-b2aa-e50192c4dfbb.png)

## Steps to run locally
### Prerequisites
- install local [Postgres server](https://www.postgresql.org/download/). You can also `brew install postgresql@15` and then add the following to your `~/.zshrc` or `~/.zprofile`, etc:
```sh
export LDFLAGS="-L/opt/homebrew/opt/postgresql@15/lib"
export CPPFLAGS="-I/opt/homebrew/opt/postgresql@15/include"
export PATH="/opt/homebrew/opt/postgresql@15/bin:$PATH"
```
- make sure you have libpq installed: `brew install libpq`, and in your profile, add `export PATH="/opt/homebrew/opt/libpq/bin:$PATH"`. If this doesn't work, try `brew link --force libpq`.

- install Diesel CLI with `cargo install diesel_cli --no-default-features --features postgres`, refer to [Diesel Getting Started guide](https://diesel.rs/guides/getting-started) for more details
- [optional but handy] Postgres client like [Postico](https://eggerapps.at/postico2/), for local check, query execution etc.

### Start the Postgres Service

Postgres must run as a service in the background for other tools to communicate with.  If it was installed using homebrew, it can be started as a service with:

``` sh
brew services start postgresql@version
```

### Local Development(Recommended)

See the [docs](https://docs.sui.io/guides/developer/getting-started/local-network) for detailed information. Below is a quick start guide:

Start a local network using the `sui` binary:
```sh
cargo run --bin sui -- start --with-faucet --force-regenesis
```

If you want to run a local network with the indexer enabled (note that `libpq` is required), you can run the following command after following the steps in the next section to set up an indexer DB:
```sh
cargo run --bin sui -- start --with-faucet --force-regenesis --with-indexer --pg-port 5432 --pg-db-name sui_indexer_v2
```

### Running standalone indexer
1. DB setup, under `sui/crates/sui-indexer` run:
```sh
# an example DATABASE_URL is "postgres://postgres:postgres@localhost/exampledb"
diesel setup --database-url="<DATABASE_URL>"
diesel database reset --database-url="<DATABASE_URL>"
```
Note that you need an existing database for this to work. Using the DATABASE_URL example in the comment of the previous code, replace `exampledb` with the name of your database.

2. Checkout to your target branch

For example, if you want to be on the DevNet branch
```sh
git fetch upstream devnet && git reset --hard upstream/devnet
```
3. Start indexer binary, under `sui/crates/sui-indexer` run:
- run indexer as a writer, which pulls data from fullnode and writes data to DB
```sh
# Change the RPC_CLIENT_URL to http://0.0.0.0:9000 to run indexer against local validator & fullnode
cargo run --bin sui-indexer -- --db-url "<DATABASE_URL>" --rpc-client-url "https://fullnode.devnet.sui.io:443" --fullnode-sync-worker --reset-db
```
- run indexer as a reader, which is a JSON RPC server with the [interface](https://docs.sui.io/sui-api-ref#suix_getallbalances)
```
cargo run --bin sui-indexer -- --db-url "<DATABASE_URL>" --rpc-client-url "https://fullnode.devnet.sui.io:443" --rpc-server-worker
```
More flags info can be found in this [file](src/main.rs#L41).

### DB reset
When making db-related changes, you may find yourself having to run migrations and reset dbs often. The commands below are how you can invoke these actions.
```sh
cargo run --bin sui-indexer -- --database-url "<DATABASE_URL>" reset-database --force
```

## Steps to run locally (TiDB)

### Prerequisites

1. Install TiDB

``` sh
curl --proto '=https' --tlsv1.2 -sSf https://tiup-mirrors.pingcap.com/install.sh | sh
```

2. Install a compatible version of MySQL (At the time of writing, this is MySQL 8.0 -- note that 8.3 is incompatible).

``` sh
brew install mysql@8.0
```

3. Install a version of `diesel_cli` that supports MySQL (and probably also Postgres). This version of the CLI needs to be built against the version of MySQL that was installed in the previous step (compatible with the local installation of TiDB, 8.0.37 at time of writing).

``` sh
MYSQLCLIENT_LIB_DIR=/opt/homebrew/Cellar/mysql@8.0/8.0.37/lib/ cargo install diesel_cli --no-default-features --features postgres --features mysql --force
```

### Run the indexer

1.Run TiDB

```sh
tiup playground
```

2.Verify tidb is running by connecting to it using the mysql client, create database `test`

```sh
mysql --comments --host 127.0.0.1 --port 4000 -u root
create database test;
```

3.DB setup, under `sui/crates/sui-indexer` run:

```sh
# an example DATABASE_URL is "mysql://root:password@127.0.0.1:4000/test"
diesel setup --database-url="<DATABASE_URL>" --migration-dir='migrations/mysql'
diesel database reset --database-url="<DATABASE_URL>" --migration-dir='migrations/mysql'
```

Note that you need an existing database for this to work. Using the DATABASE_URL example in the comment of the previous code, replace `test` with the name of your database.
4. Run indexer as a writer, which pulls data from fullnode and writes data to DB

```sh
# Change the RPC_CLIENT_URL to http://0.0.0.0:9000 to run indexer against local validator & fullnode
cargo run --bin sui-indexer --features mysql-feature --no-default-features -- --db-url "<DATABASE_URL>" --rpc-client-url "https://fullnode.devnet.sui.io:443" --fullnode-sync-worker --reset-db
```

### Extending the indexer

To add a new table, run `diesel migration generate your_table_name`, and modify the newly created `up.sql` and `down.sql` files.

You would apply the migration with `diesel migration run`, and run the script in `./scripts/generate_indexer_schema.sh` to update the `schema.rs` file.
