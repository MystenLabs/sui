# Sui Wallet Adapter [WIP]

This project is an adapter for wallets on the Sui blockchain.

## Overall Breakdown:
#### packages/adapters/base-adapter
Defines the base interfaces that wallets must implement to integrate with the Sui Wallet Adapter.

#### packages/adapters/integrations/*
Contains all the wallet integrations into the Sui Wallet Adapter

#### packages/adapters/integrations/all-wallets
This module exports all wallet integrations for the Sui Wallet Adapter. It may be convenient to import this if planning to integrate multiple wallets.

#### packages/react-providers
This module contains a provider that injects functions that allow interactions with the wallet. See the example provided in src/ to see how this can be integrated into your react app.

#### packages/ui
This module contains basic UI components such as the 'Connect/Manage Wallet' button and any corresponding modals. See the example provided in src/ to see how this can be integrated into your react app.

