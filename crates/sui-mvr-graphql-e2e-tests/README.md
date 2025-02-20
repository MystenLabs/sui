End-to-end tests for GraphQL service, built on top of the transactional test
runner.

# Local Set-up

These tests require a running instance of the `postgres` service, with a
database set-up. The instructions below assume that `postgres` has been
installed using `brew`:

1. See the instructions in the Sui Indexer [README](../sui-indexer/README.md)
   for pre-requisites and starting the Postgres service.

2. When postgres is initially installed, it creates a role for your current
   user.  We need to use that role to create the role that will access the
   database:

```sh
$ ME=$(whoami)
$ psql "postgres://$ME:$ME@localhost:5432/postgres" \
    -c "CREATE ROLE postgres WITH SUPERUSER LOGIN PASSWORD 'postgrespw';"
```

3. Then, create the database that the tests expect, using the `postgres` user
   and increase the max connections since many tests might run in parallel.

```sh
$ psql "postgres://postgres:postgrespw@localhost:5432/postgres" \
    -c "CREATE DATABASE sui_indexer_v2;" -c "ALTER SYSTEM SET max_connections = 500;"
```

4. Finally, restart the `postgres` server so the max connections change takes
   effect.

Mac
```sh
brew services restart postgresql@15

```

Linux
```sh
/etc/init.d/postgresql restart
```

# Running Locally

```sh
$ cargo nextest run
```

# Snapshot Stability

Tests are pinned to an existing protocol version that has already been used on a
production network. The protocol version controls the protocol config and also
the version of the framework that gets used by tests. By using a version that
has already been used in a production setting, we are guaranteeing that it will
not be changed by later modifications to the protocol or framework (this would
be a bug).

When adding a new test, **remember to set the `--protocol-version`** for that
test to ensure stability.
