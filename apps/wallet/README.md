# Sui Wallet

A Chrome extension wallet for [Sui](https://sui.io).

# Set Up

**Requirements**: Node 14.0.0 or later.

Dependencies are managed using [`pnpm`](https://pnpm.io/). You can start by installing dependencies in the root of the Sui repository:

```
$ pnpm install
```

> All `pnpm` commands are intended to be run in the root of the Sui repo. You can also run them within the `apps/wallet` directory, and remove change `pnpm wallet` to just `pnpm` when running commands.

## Build in watch mode (dev)

To build the extension and watch for changes run:

```
pnpm wallet start
```

This will build the app in the [dist/](./dist/) directory, watch for changes and rebuild it. (Also runs prettier to format the files that changed.)

## Environment Variables

You can config default network and RPC endpoints by copying [configs/environment/.env.defaults](configs/environment/.env.defaults) and rename it to `configs/environment/.env`.

For example, to change the default network from DevNet to Local Network, you can change `API_ENV=devNet` to `API_ENV=local`.

## Build once in dev mode

To build the app once in development mode (no optimizations etc) run

```
pnpm wallet build:dev
```

The output directory is the same [dist/](./dist/), all build artifacts will go there

## Build once in prod mode

To build the app once in production mode run

```
pnpm wallet build:prod
```

Same as above the output is [dist/](./dist/).

## Install the extension to Chrome

After building the app, the extension needs to be installed to Chrome. Follow the steps to [load an unpacked extension](https://developer.chrome.com/docs/extensions/mv3/getstarted/#unpacked) and install the app from the [dist/](./dist/) directory.

## Testing

```
pnpm wallet test
```
