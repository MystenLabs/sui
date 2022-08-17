# Sui Wallet Adapter

This project is an adapter for wallets on the Sui blockchain.

### Demo App

To run the demo app,

```
pnpm install
pnpm start
```

### Modules

#### packages/adapters/base-adapter

Defines the base interface in which the wallet adapters will implement.

#### packages/adapters/integrations

Contains the integrations for each wallet. If you are importing multiple wallets, it may be beneficial to instead import the `all-wallets` module, which exports all integrated wallet adapter interfaces.

#### packages/ui

Contains basic UI elements such as the Connect/Manage Wallet and corresponding modals for each wallet. Check out ./src for an example of how to integrate this to your project.

#### packages/react-providers

Contains the react providers that inject the endpoint defined in `packages/adapters/base-adapter` into your project. Check out ./src for an example of how to integrate this to your project.
