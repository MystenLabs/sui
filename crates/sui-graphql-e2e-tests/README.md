End-to-end tests for GraphQL service, built on top of the transactional test
runner.

# Local Set-up

These tests require a running instance of the `postgres` service, with a
database set-up.  The instructions below assume that `postgres` has been
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

3. Then, create the database that the tests expect, using the `postgres` user:

```sh
$ psql "postgres://postgres:postgrespw@localhost:5432/postgres" \
    -c "CREATE DATABASE sui_indexer_v2;"
```

# Running Locally

When running the tests locally, they need to be run serially (one at a time),
and with the `pg_integration` feature enabled:

```sh
$ cargo nextest run -j 1 --features pg_integration
```
