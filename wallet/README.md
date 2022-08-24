# Sui Wallet

A Chrome (v88+) extension wallet for [Sui](https://sui.io).

# Set Up

**Requirements**: Node 14.0.0 or later.

Dependencies are managed using [`pnpm`](https://pnpm.io/). You can start by installing dependencies in the root of the Sui repository:

```
$ pnpm install
```

Currently the Wallet depends on an unreleased version of `sui.js`, the TypeScript SDK for Sui. Therefore, you need to build the SDK first:

```bash
$ pnpm --filter @mysten/sui.js... build
```

> **Note:** If you are updating the SDK and Explorer at the same time, you need to re-build the SDK whenever you makes changes for it to be reflected in the Explorer.

## Build in watch mode (dev)

To build the extension and watch for changes run:

```
pnpm start
```

This will build the app in the [dist/](./dist/) directory, watch for changes and rebuild it. (Also runs prettier to format the files that changed.)

## Build once in dev mode

To build the app once in development mode (no optimizations etc) run

```
pnpm run build:dev
```

The output directory is the same [dist/](./dist/), all build artifacts will go there

## Build once in prod mode

To build the app once in production mode run

```
pnpm run build:prod
```

Same as above the output is [dist/](./dist/).

## Install the extension to Chrome

After building the app, the extension needs to be installed to Chrome. Follow the steps to [load an unpacked extension](https://developer.chrome.com/docs/extensions/mv3/getstarted/#unpacked) and install the app from the [dist/](./dist/) directory.

## Testing

```
pnpm run test
```
