# Sui Wallet

A chrome (v88+) extension wallet for [Sui](https://sui.io).

# Set up

**Requirements**: Node 14.0.0 or later.

Run `npm i` first to install the required `node modules`

Then one of the following build steps is required:

## Build in watch mode (dev)

To build the extension and watch for changes run

```
npm start
```

This will build the app in the [dist/](./dist/) directory, watch for changes and rebuild it. (Also runs prettier to format the files that changed.)

## Build once in dev mode

To build the app once in development mode (no optimizations etc) run

```
npm run build:dev
```

The output directory is the same [dist/](./dist/), all build artifacts will go there

## Build once in prod mode

To build the app once in production mode run

```
npm run build:prod
```

Same as above the output is [dist/](./dist/).

## Install the extension to Chrome

After building the app, the extension needs to be installed to Chrome. Follow the steps [here](https://developer.chrome.com/docs/extensions/mv3/getstarted/#unpacked) and install the app from the [dist/](./dist/) directory.
