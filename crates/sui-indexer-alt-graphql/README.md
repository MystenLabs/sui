# sui-indexer-alt-graphql

A GraphQL service that answers queries based on indexed data.

## Set-up

GraphQL requires access to one or more Data Sources (see below), so these need
to be set-up and accessible for GraphQL to run.

## Data Sources

### Postgres

GraphQL can run with access to just a postgres database, written to by
`sui-indexer-alt`, as long as all the required tables are present (all
pipelines are enabled on the indexer). See the indexer's
[README](../sui-indexer-alt/README.md) for details on how to set it up.

### (optional) Bigtable

If GraphQL is given access to credentials for Bigtable, it will look there to
answer key-value queries (fetching an object by its ID and version, a
checkpoint by its sequence number, or a transaction by its digest), instead of
the relevant postgres tables.

### (TBD) Consistent Store

The consistent store is used to answer queries about the live object set (live
objects by owner or by type, balances per address, etc) parametrized by a
recent checkpoint, but is not yet ready or integrated into GraphQL Beta.

### (TBD) Fullnode

Fullnodes are used to execute and simulate transactions, but are not yet
integrated into GraphQL Beta.

## Tests

Tests require postgres to be installed (but not necessarily running), and
benefit from the following tools:

```sh
cargo install cargo-insta   # snapshot testing utility
cargo install cargo-nextest # better test runner
```

The following tests are related to GraphQL (**run from the root of the repo**):
```sh
cargo nextest run -p sui-indexer-alt-graphql
cargo nextest run -p sui-indexer-alt-reader
cargo nextest run -p sui-indexer-alt-e2e-tests -- graphql
```

The first test is GraphQL's own unit tests, the second tests code related to
database access (this code is shared among RPCs that read from
`sui-indexer-alt`'s stores), the final test is an end-to-end test suite that
also tests the indexer.

### Re-generating the schema

If a change affects the GraphQL schema, it will need to be re-generated. This
is done automatically as part of test runs, and can be isolated as follows:

```sh
cargo nextest run -p sui-indexer-alt-graphql -- schema_export
cargo nextest run -p sui-indexer-alt-graphql --features staging -- schema_export
cargo insta review
```

Note that there are two schemas (production and staging), and both need to be
regenerated.

This operation is also run by CI, so stale schemas will be detected at diff
time.

## Configuration

GraphQL is mostly configured through a TOML file, a copy of the default config
can be generated using the following command, but it can be omitted if none of
the defaults need to be changed:

```sh
cargo run --bin sui-indexer-alt-graphql -- generate-config > graphql_config.toml
```

GraphQL also needs access to the configuration of the indexer(s) that are
writing to the database it is reading from. It uses these to understand which
pipelines are being populated and therefore what features should be available
in the database.

## Running

The service can be run with the following minimal command:

```sh
cargo run --bin sui-indexer-alt-graphql -- rpc \
  --indexer-config indexer_alt_config.toml
```

In this configuration, the RPC will respond at

- `http://localhost:7000/graphql` POST requests will be treated as GraphQL
  queries, GET requests will be routed to a Web IDE.
- `http://localhost:7000/health` a simple health check endpoint that returns
  200 OK if the service is running, can talk to its stores and the data is not
  too stale.
- `http://localhost:9184/metrics` for Prometheus metrics.

It will try and connect to the indexer's default postgres database and route
all its queries there (no Bigtable, fullnode, or consistent store access).
