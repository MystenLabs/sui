# Sui Wallet

A Chrome extension wallet for [Sui](https://sui.io).

# Set Up

**Requirements**: 18.0.0 or later.

Dependencies are managed using [`pnpm`](https://pnpm.io/). You can start by installing dependencies in the root of the Sui repository:

```
$ pnpm install
```

> All `pnpm` commands below are intended to be run in the root of the Sui repo.

## Build in watch mode (dev)

To build the extension and watch for changes run:

```
pnpm wallet dev
```

This will build the app in the [dist/](./dist/) directory, watch for changes and rebuild it. (Also runs prettier to format the files that changed.)

## Environment Variables

You can config default network and RPC endpoints by copying [configs/environment/.env.defaults](configs/environment/.env.defaults) and rename it to `configs/environment/.env`.

For example, to change the default network from DevNet to Local Network, you can change `API_ENV=devNet` to `API_ENV=local`.

## Building the wallet

To build the app, run the following command:

```
pnpm wallet build
```

The output directory is the same [dist/](./dist/), all build artifacts will go there

## Install the extension to Chrome

After building the app, the extension needs to be installed to Chrome. Follow the steps to [load an unpacked extension](https://developer.chrome.com/docs/extensions/get-started/tutorial/hello-world#load-unpacked) and install the app from the [dist/](./dist/) directory.

## Testing

```
pnpm wallet test
```
