---
title: Requesting Gas Tokens from Faucet
---

Sui faucet is a helpful tool where Sui developers can get free test SUI tokens to deploy and interact with their programs on Sui's DevNet and TestNet.

Test tokens can be requested in the following ways:

## Pre-requisites

To request tokens from the faucet, one must own a wallet address that can receive the SUI tokens. An address can be generated via the [Sui CLI tool](https://docs.sui.io/devnet/build/cli-client#active-address) or the [Sui wallet](https://docs.sui.io/devnet/explore/wallet-browser).

## 1. Request test tokens through Discord

1. Join [Discord](https://discord.gg/sui).
   If you try to join the Sui Discord channel using a newly created Discord account you may need to wait a few days for validation.
1. Request test SUI tokens in the Sui [#devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130) or [#testnet-faucet](https://discord.com/channels/916379725201563759/1037811694564560966) Discord channel. Note that the TestNet faucet is only available when TestNet is live.
   Send the following message to the channel with your client address:
   !faucet <Your client address>

## 2. Request test tokens through wallet

You can request test tokens within the [Sui wallet](https://docs.sui.io/devnet/explore/wallet-browser#add-sui-tokens-to-your-sui-wallet).

Note: This option will be disabled for TestNet in TestNet Wave 2. Please use Discord channel instead for TestNet Wave 2.

## 3. Request test tokens through Curl

You can also use the following Curl command to request tokens directly from the faucet server.

```
curl --location --request POST 'https://faucet.devnet.sui.io/gas' \
--header 'Content-Type: application/json' \
--header 'Cookie: __cf_bm=DZ4EG6GULlrwnyZMGoqwFjD8p6trJzWsY0LxvHd.NJs-1674598151-0-ARjqPuQjq1efkiQX6ItAI/4QejXhgHfA5rgr8oNoKiRskODMvTraH7VHGx7PF7IjgvEJTbIRB52Yia/Z6UfVlpo=; _cfuvid=hwMbc_CMbJrDSx2dM9tblANlIrpdLoCGFTbOdAhl4HM-1674172742724-0-604800000' \
--data-raw '{
    "FixedAmountRequest": {
        "recipient": "<YOUR SUI ADDRESS>"
    }
}'
```

Replace `'https://faucet.devnet.sui.io/gas'` with `http://127.0.0.1:5003/gas` when working with local network.

Note: This option will be disabled for TestNet in TestNet Wave 2. Please use Discord channel instead for TestNet wave 2.

## 4. Request test tokens through TypeScript SDK

You can also access the faucet through the TS-SDK.

```
import { JsonRpcProvider, Network } from '@mysten/sui.js';
// connect to Devnet
const provider = new JsonRpcProvider(Network.DEVNET);
// get tokens from the DevNet faucet server
await provider.requestSuiFromFaucet(
  '<YOUR SUI ADDRESS>'
);
```

Related topics:

- [Connect to Sui Devnet](../build/devnet.md).
