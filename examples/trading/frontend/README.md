# Trading e2e demo - Frontend

This dApp was created using `@mysten/create-dapp` that sets up a basic React
Client dApp.

## First Steps

Before running the frontend, it's recommended that you follow the API setup to
[publish the contracts](../api/README.md) (or re-use the published ones).

### Demo Contracts

The following packages are published and used for demo purposes on Testnet.

For `escrow-contract.json` file:

```json
{
  "packageId": "0xead655f291ed9e1f5cac3bc4b2cfcccec91502940c0ba4d846936268964524c9"
}
```

For `demo-contract.json` file:

```json
{
  "packageId": "0x164183829178d7620595919907d35bd3800b4345152f793594af8b2ba252d58a"
}
```

### Constants

You can change package addresses, the api endpoint, etc, on the `constants.ts`
file.

## Starting the dApp

To install dependencies you can run

```bash
pnpm install --ignore-workspace
```

To start your dApp in development mode run

```bash
pnpm dev
```

## Building

To build your app for deployment you can run

```bash
pnpm build
```
