---
title: Connect to Sui Devnet
---

Use the Sui Devnet network to experiment with the version of Sui running on the Devnet network.

The Sui Devnet network consists of:

 * Four validator nodes operated by Mysten Labs. Clients send transactions and read requests via this endpoint: `https://fullnode.devnet.sui.io:443` using [JSON-RPC](../build/json-rpc.md).
 * A public network [Sui Explorer](https://explorer.sui.io) for browsing transaction history.

You can [request test SUI tokens](#request-test-tokens) through the Sui [devnet-faucet](https://discordapp.com/channels/916379725201563759/971488439931392130) Discord channel. These coins have no financial value. With each Sui release, the network resets and removes all assets (coins and NFTs).

See announcements about Sui Devnet in the [#devnet-updates](https://discord.com/channels/916379725201563759/1004638487078772736) Discord channel.

See the [terms of service](https://sui.io/terms/) for using the Devnet network.

## Tools

Sui provides the following tools to interact with Sui Devnet:

 * [Sui command line interface (CLI)](../build/cli-client.md)
     * create and manage your private keys
     * create example NFTs
     * call and publish Move modules
 * [Sui Explorer](https://github.com/MystenLabs/sui/blob/main/apps/explorer/README.md) to view transactions and objects on the network

## Environment set up

First, [Install Sui](../build/install.md#install-sui-binaries). After you install Sui, [request SUI test tokens](#request-gas-tokens) through [Discord](https://discordapp.com/channels/916379725201563759/971488439931392130).

To check whether Sui is already installed, run the following command:

```shell
which sui
```

If Sui is installed, the command returns the path to the Sui binary. If Sui is not installed, it returns `sui not found`.

See the [Sui Releases](https://github.com/MystenLabs/sui/releases) page to view the changes in each Sui release.

## Configure Sui client

If you previously ran `sui genesis -f` to create a local network, it created a Sui client configuration file (client.yaml) that connects to `localhost` `http://0.0.0.0:9000`. See [Connect to custom RPC endpoint](#connect-to-custom-rpc-endpoint) to update the client.yaml file.

To connect the Sui client to Sui Devnet, run the following command:
```shell
sui client
```

The first time you start Sui client, it displays the following message:

```
Config file ["/Users/dir/.sui/sui_config/client.yaml"] doesn't exist, do you want to connect to a Sui Full node server [y/n]?
```

Press **y** and then press **Enter**. It then asks for the RPC server URL: 

```
Sui Full node server URL (default to Sui Devnet if not specified) :
```

Press **Enter** to connect to Sui Devnet. To use a custom RPC server, enter the URL to the custom RPC endpoint and then press **Enter**.

```
Select key scheme to generate keypair (0 for ed25519, 1 for secp256k1, 2 for secp256r1):
```

Press **0**, **1**, or **2** to select a key scheme.

Sui returns a message similar to the following (depending on the key scheme you selected) that includes the address and recovery phrase for the address:

```
Generated new keypair for address with scheme "secp256r1" [0xb9c83a8b40d3263c9ba40d551514fbac1f8c12e98a4005a0dac072d3549c2442]
Secret Recovery Phrase : [cap wheat many line human lazy few solid bored proud speed grocery raise erode there idea inform culture cousin shed sniff author spare carpet]
```

### Connect to a custom RPC endpoint

If you previously used `sui genesis` with the force option (`-f` or `--force`), your client.yaml file already includes two RPC endpoints: localnet `http://0.0.0.0:9000` and devnet `https://fullnode.devnet.sui.io:443`). You can view the defined environments with the `sui client envs` command, and switch between them with the `sui client switch` command.

If you previously installed a Sui client that connected to the Devnet network, or created a local network, you can modify your existing `client.yaml` to change the configured RPC endpoint:

To add a custom RPC endpoint, run the following command. Replace values in `<` `>` with values for your installation:

```shell
sui client new-env --alias <ALIAS> --rpc <RPC>
```

To switch the active network, run the following command:
```shell
sui client switch --env <ALIAS>
```

If you encounter an issue, delete the Sui configuration directory (`~/.sui/sui_config`) and reinstall the latest [Sui binaries](../build/install.md#install-sui-binaries).

## Validating

Note that in the following sections, the object ID's, addresses, and authority signatures used are example values only. Sui generates unique values for each of these, so you see different values when you run the commands.

## Request test tokens

 1. Join [Discord](https://discord.gg/sui). 
   If you try to join the Sui Discord channel using a newly created Discord account you may need to wait a few days for validation. 
 1. Get your Sui client address: `sui client active-address`
 1. Request test SUI tokens in the Sui [#devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130) Discord channel.
  Send the following message to the channel with your client address:
  !faucet <Your client address>

## Mint an example NFT

To create a Non-Fungible Token (NFT), run:
```shell
sui client create-example-nft
```

The command returns a response similar to the following:
```
Successfully created an ExampleNFT:

----- 0x2::devnet_nft::DevNetNFT (0xbacc053ad6e55084bd400cb0479533805ad2cfac33e085a3cc6b7364fcbde953[0x2]) -----
Owner: Account Address ( 0xb9c83a8b40d3263c9ba40d551514fbac1f8c12e98a4005a0dac072d3549c2442 )
Version: 0x2
Storage Rebate: 29
Previous Transaction: TransactionDigest(B5gTaoPCTef4rDjVRoD8C7QpJebiWtfc9eigHxqHNShb)
----- Data -----
type: 0x2::devnet_nft::DevNetNFT
description: An NFT created by the Sui Command Line Tool
id: 0xbacc053ad6e55084bd400cb0479533805ad2cfac33e085a3cc6b7364fcbde953
name: Example NFT
url: ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty
```

To view the created object in [Sui Explorer](https://explorer.sui.io), append the object ID to the following URL https://explorer.sui.io/objects/.

The following command demonstrates how to customize the name, description, or image of the NFT:
```shell
sui client create-example-nft --url=https://user-images.githubusercontent.com/76067158/166136286-c60fe70e-b982-4813-932a-0414d0f55cfb.png --description="The greatest chef in the world" --name="Greatest Chef"
```

The command returns a new object ID:
```
Successfully created an ExampleNFT:

----- 0x2::devnet_nft::DevNetNFT (0xa80a070133bfe7330eb8c02f5d91aaa9a6afe630eeb8b9ef9be08725642a02e1[0x3]) -----
Owner: Account Address ( 0xb9c83a8b40d3263c9ba40d551514fbac1f8c12e98a4005a0dac072d3549c2442 )
Version: 0x3
Storage Rebate: 32
Previous Transaction: TransactionDigest(9hTv1Nme1C1PLw7kXhaksJfRFX65VovJm78xaDkp7c4R)
----- Data -----
type: 0x2::devnet_nft::DevNetNFT
description: The greatest chef in the world
id: 0xa80a070133bfe7330eb8c02f5d91aaa9a6afe630eeb8b9ef9be08725642a02e1
name: Greatest Chef
url: https://user-images.githubusercontent.com/76067158/166136286-c60fe70e-b982-4813-932a-0414d0f55cfb.png
```

To view details about the object in Sui Explorer, copy the object ID and search for it in the Explorer search field.

## Publish a Move module

This section describes how to publish a sample Move package using code developed in the [Sui Move tutorial](../build/move/write-package.md). The instructions assume that you installed Sui in the default location.
```shell
sui client publish <your-sui-repo>/sui_programmability/examples/move_tutorial --gas-budget 30000
```

The response resembles the following:
```shell
----- Certificate ----
Transaction Hash: TransactionDigest(3a6fv6mde6U4xvT4wxJao8qnCBjKEuSqYgpXPDW8mCjj)
Transaction Signature: [Signature(AA==@VmcFxKAwZszgLfgakdpAIbQGasp0pLHWuaLoOCFWGzaY6+FBgicyr65fD90Fa/9qQF/o7QXYDqVV1QxceJ9JDw==@aNxLU5gVv2cahhUeZ7Ig6IduqqFGZB/ULs8OkUoCgBo=)]
Signed Authorities Bitmap: RoaringBitmap<[0, 1, 3]>
Transaction Kind : Publish
Sender: 0x0727340240175d8672aee86d611e3c1a1b1ea9a09158ada0534d1e0bf675a7b4
Gas Payment: Object ID: 0x4c12717f8ce04303ec6772ae2cdf7be797a3d495bb0e62b82e463df2e1795812, version: 0x1f60, digest: o#JS+wEZt+ZfLbiYD7893GWBXhnPhG9tPRZE7rmfx5NP8=
Gas Owner: 0x0727340240175d8672aee86d611e3c1a1b1ea9a09158ada0534d1e0bf675a7b4
Gas Price: 1
Gas Budget: 1000
----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: 0x571945be15b502103ca17c91a07bd7624f5748c1bcdab1d86702b97c9c654f20 , Owner: Immutable
  - ID: 0x8c444146fddedca190133b4c1b91ce327d682e5432d6ea1381099300790c525f , Owner: Account Address ( 0x0727340240175d8672aee86d611e3c1a1b1ea9a09158ada0534d1e0bf675a7b4 )
Mutated Objects:
  - ID: 0x4c12717f8ce04303ec6772ae2cdf7be797a3d495bb0e62b82e463df2e1795812 , Owner: Account Address ( 0x0727340240175d8672aee86d611e3c1a1b1ea9a09158ada0534d1e0bf675a7b4 )
```

The package publish operation does two important things:

* Creates a package object (with ID `0x571945be15b502103ca17c91a07bd7624f5748c1bcdab1d86702b97c9c654f20`)
* Creates a `Forge` object (with ID `0x8c444146fddedca190133b4c1b91ce327d682e5432d6ea1381099300790c525f`) as a result of running a [module initializer](../build/move/debug-publish.md#module-initializers) for the one (and only) module of this package.

You can check the details of each object using the `sui client object <OBJECT_ID>` command. For example, checking the forge object returns:

```shell
----- Move Object (0x8c444146fddedca190133b4c1b91ce327d682e5432d6ea1381099300790c525f[8033]) -----
Owner: Account Address ( 0x0727340240175d8672aee86d611e3c1a1b1ea9a09158ada0534d1e0bf675a7b4 )
Version: 8033
Storage Rebate: 13
Previous Transaction: TransactionDigest(3a6fv6mde6U4xvT4wxJao8qnCBjKEuSqYgpXPDW8mCjj)
----- Data -----
type: 0x571945be15b502103ca17c91a07bd7624f5748c1bcdab1d86702b97c9c654f20::my_module::Forge
id: 0x8c444146fddedca190133b4c1b91ce327d682e5432d6ea1381099300790c525f
swords_created: 0
```

When you publish a package, the IDs for the objects created are different than the ones displayed in this example. This remainder of this topic uses <PACKAGE_ID> and <FORGE_ID> to represent the IDs for the created objects. You must replace these with the values for your package.

## Make a Move call

This section describes how to call into functions defined in the package published in the previous section. Use the (<PACKAGE_ID>) and (<FORGE_ID>) values from your package to  create swords and transfer them to other players.

To demonstrate this, <PLAYER_ADDRESS> represents the address of the player to receive a sword. You can use an address of someone you know, or create another address for testing with the following Sui Client CLI command:
```shell
sui client new-address ed25519
```

The command returns the following message and a 24-word recovery phrase for the address:

```
Created new keypair for address with scheme Secp256k1: [0xbaab7e8bda187e69fd402b6b5dbfda35b3baa7a2d71e3f1c58d698ae8f13ba88]
Secret Recovery Phrase : [put error net quiz afraid dune cheese update polar define grape canyon give fresh satisfy arm wrong seed cry neutral heart august start now]
```

To create a sword and transfer it to another player, we use the following command to call the `sword_create` [function](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/move_tutorial/sources/my_module.move#L47) in the `my_module` [module](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/move_tutorial/sources/my_module.move#L4) of the package we previously published.

You must use the same format for the command and function parameters as the example shown.

```shell
sui client call --function sword_create --module my_module --package 0x<PACKAGE_ID> --args \"0x<FORGE_ID>\" 42 7 \"0x<PLAYER_ADDRESS>\" --gas-budget 30000
```

Go to the Sui Explorer to observe a newly created object. You should see a sword object created with `Magic` property of `42` and `Strength` property of `7` and transferred to the new owner.

Replace the object ID in the Explorer with the object ID of the created object you observed in your own command output, appended to:
https://explorer.sui.io/objects/

Related topics:
 * [Create Smart Contracts with Move](../build/move).
 * [Programming with Objects](../build/programming-with-objects/)
