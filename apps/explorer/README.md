# Sui Explorer Frontend

[Sui Explorer](https://explorer.sui.io/) is a network explorer for the Sui network, similar in functionality to [Etherscan](https://etherscan.io/) or [Solana Explorer](https://explorer.solana.com/). Use Sui Explorer to see the latest transactions and objects.

# Set Up

**Requirements**: Node 14.0.0 or later.

Dependencies are managed using [`pnpm`](https://pnpm.io/). You can start by installing dependencies in the root of the Sui repository:

```
$ pnpm install
```

# How to Switch Environment

By default, the Sui Explorer attempts to connect to a local RPC server. For more information about using a local RPC server, see [Local RPC Server & JSON-RPC API Quick Start](../../doc/src/build/json-rpc.md).

If you want to use the explorer with another network, you can select your preferred network in the header of the explorer.

## Connecting to the Static Data

The Sui Explorer can also connect to a local, static JSON dataset that can be found at `./src/utils/static/mock_data.json` and `./src/utils/static/owned_object.json`.

For example, suppose we wish to locally run the website using the static JSON dataset and not the API, then we could run the following:

```bash
pnpm dev:static

```

## To run end-to-end localnet test

Start validators locally:

```bash
cargo run --bin sui-test-validator
```

In a a separate terminal, start explorer:

```bash
pnpm explorer dev
```

In a separate terminal, run cypress locally, this should open a chrome simulating all tests:

```bash
pnpm explorer exec cypress open
```

# Other pnpm commands

### `pnpm test`

This runs a series of end-to-end browser tests using the website as connected to the static JSON dataset. This command is run by the GitHub checks. The tests must pass before merging a branch into main.

### `pnpm build`

Builds the app for production to the `build` folder.

It bundles React in production mode and optimizes the build for the best performance.

### `pnpm lint`

Run linting check (prettier/eslint/stylelint).

### `pnpm lint:fix`

Run linting check but also try to fix any issues.

# Features

Currently the Explorer supports

-   Landing page with latest transactions
-   Transaction details page
-   Object details page
-   Address page with owned objects
-   Search for transactions, addresses, and Objects by ID

See [Experiment with Sui DevNet](https://docs.sui.io/build/devnet) for use.
