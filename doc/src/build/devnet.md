---
title: Connect to Sui Devnet
---

Use the Sui Devnet network to experiment with Sui. Please submit feedback about your experience using Devnet, report bugs, and contribute to Sui.

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

First, [Install Sui](../build/install.md#install-or-update-sui-binaries). After you install Sui, [request SUI test tokens](#request-gas-tokens) through [Discord](https://discordapp.com/channels/916379725201563759/971488439931392130).

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

```shell
Config file ["/Users/dir/.sui/sui_config/client.yaml"] doesn't exist, do you want to connect to a Sui RPC server [y/n]?
```
Press **y** and then press **Enter**. It then asks for the RPC server URL: 

```shell
Sui RPC server Url (Default to Sui Devnet if not specified) :
```
Press **Enter** to connect to Sui Devnet. To use a custom RPC server, enter the URL to the RPC endpoint to use.

```shell
Select key scheme to generate keypair (0 for ed25519, 1 for secp256k1):
```
Type **0** or **1** to select key scheme.

### Connect to custom RPC endpoint

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

If you encounter an issue, delete the Sui configuration directory (`~/.sui/sui_config`) and reinstall the latest [Sui binaries](../build/install.md#install-or-update-sui-binaries).

## Validating

Note that in the following sections, the object ID's, addresses, and authority signatures used are example values only. Sui generates unique values for each of these, so you see different values when you run the commands.

## Request test tokens

1. Join [Discord](https://discord.gg/sui). 
   If you try to join the Sui Discord channel using a newly created Discord account you may need to wait a few days for validation. 
1. Get your Sui client address:
   ```shell
   sui client active-address
   ```
1. Request test SUI tokens in the Sui [#devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130) Discord channel.
  Send the following message to the channel with your client address:
  !faucet <Your client address>

## Mint an example NFT

To create a Non-Fungible Token (NFT), run:
```shell
sui client create-example-nft
```

The command returns a response similar to the following:
```shell
Successfully created an ExampleNFT:

ID: ED883F6812AF447B9B0CE220DA5EA9E0F58012FE
Version: 1
Owner: Account Address ( 9E9A9D406961E478AA80F4A6B2B167673F3DF8BA )
Type: 0x2::devnet_nft::DevNetNFT
```

The preceding command created an object with ID `ED883F6812AF447B9B0CE220DA5EA9E0F58012FE`. Use the Sui Client CLI to [view objects owned by the address](../build/cli-client.md#view-objects-an-address-owns).

To view the created object in [Sui Explorer](https://explorer.sui.io), append the object ID to the following URL https://explorer.sui.io/objects/.

The following command demonstrates how to customize the name, description, or image of the NFT:
```shell
$ sui client create-example-nft --url=https://user-images.githubusercontent.com/76067158/166136286-c60fe70e-b982-4813-932a-0414d0f55cfb.png --description="The greatest chef in the world" --name="Greatest Chef"
```

The command returns a new object ID:
```shell
Successfully created an ExampleNFT:

ID: EC97467A40A1305FFDEF7019C3045FBC7AA31E29
Version: 1
Owner: Account Address ( 9E9A9D406961E478AA80F4A6B2B167673F3DF8BA )
Type: 0x2::devnet_nft::DevNetNFT
```

You can view details about the object in Sui Explorer:
https://explorer.sui.io/objects/EC97467A40A1305FFDEF7019C3045FBC7AA31E29

## Publish a Move module

This section describes how to publish a sample Move package using code developed in the [Sui Move tutorial](../build/move/write-package.md).  The instructions assume that you installed Sui in the default location.
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
Sender: 0xf46dd460a0dbcc5b57deac988641a5ef29c4ab3f
Gas Payment: Object ID: 0x0c5b70eea9c634f27b0198163085748aa218f7e7, version: 0x1f60, digest: o#JS+wEZt+ZfLbiYD7893GWBXhnPhG9tPRZE7rmfx5NP8=
Gas Owner: 0xf46dd460a0dbcc5b57deac988641a5ef29c4ab3f
Gas Price: 1
Gas Budget: 1000
----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: 0x93aedf72ed5ea0905f476defb5ad329654c4f103 , Owner: Immutable
  - ID: 0xad1087d1da7fa964a9646a6eed968c9e118511dc , Owner: Account Address ( 0xf46dd460a0dbcc5b57deac988641a5ef29c4ab3f )
Mutated Objects:
  - ID: 0x0c5b70eea9c634f27b0198163085748aa218f7e7 , Owner: Account Address ( 0xf46dd460a0dbcc5b57deac988641a5ef29c4ab3f )
```

The package publish operation does two important things:

* Creates a package object (with ID `0x93aedf72ed5ea0905f476defb5ad329654c4f103`)
* Creates a `Forge` object (with ID `0xad1087d1da7fa964a9646a6eed968c9e118511dc`) as a result of running a [module initializer](../build/move/debug-publish.md#module-initializers) for the one (and only) module of this package.

You can check the details of each object using the `sui client object <OBJECT_ID>` command. For example, checking the forge object returns:

```shell
----- Move Object (0xad1087d1da7fa964a9646a6eed968c9e118511dc[8033]) -----
Owner: Account Address ( 0xf46dd460a0dbcc5b57deac988641a5ef29c4ab3f )
Version: 8033
Storage Rebate: 13
Previous Transaction: TransactionDigest(3a6fv6mde6U4xvT4wxJao8qnCBjKEuSqYgpXPDW8mCjj)
----- Data -----
type: 0x93aedf72ed5ea0905f476defb5ad329654c4f103::my_module::Forge
id: 0xad1087d1da7fa964a9646a6eed968c9e118511dc
swords_created: 0
```

When you publish a package, the IDs for the objects created are different than the ones displayed in this example. This remainder of this topic uses <PACKAGE_ID> and <FORGE_ID> to represent the IDs for the created objects. You must replace these with the values for your package.

## Make a Move call

This section describes how to call into functions defined in the package published in the previous section. Use the (<PACKAGE_ID>) and (<FORGE_ID>) values from your package to  create swords and transfer them to other players.

To demonstrate this, <PLAYER_ADDRESS> represents the address of the player to receive a sword. You can use an address of someone you know, or create another address for testing with the following Sui Client CLI command:
```shell
sui client new-address ed25519
```

The command returns the following message and a 12 word recovery phrase for the address:
```shell
Created new keypair for address with scheme Secp256k1: [0x568318261d88535009dff39779b18e1bfac59c33]
Secret Recovery Phrase : [mist drizzle rain shower downpour pond stream brook river ocean sea suinami]
```

To create a sword and transfer it to another player, we use the following command to call the `sword_create` [function](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/move_tutorial/sources/my_module.move#L47) in the `my_module` [module](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/move_tutorial/sources/my_module.move#L4) of the package we previously published.

You must use the same format for the command and function parameters as the example shown.

```shell
sui client call --function sword_create --module my_module --package 0x<PACKAGE_ID> --args \"0x<FORGE_ID>\" 42 7 \"0x<PLAYER_ADDRESS>\" --gas-budget 30000
```

The response resembles the following:
```shell
----- Certificate ----
Signed Authorities : [k#2266186afd9da10a43dd3ed73d1039c6793d2d8514db6a2407fcf835132e863b, k#1d47ad34e2bc5589882c500345c953b5837e30d6649d315c61690ba7a1e28d23, k#e9599283c0da1ac2eedeb89a56fc49cd8f3c0d8d4ddba9b0a0a5054fe7df3ffd]
Transaction Kind : Call
Package ID : 0x689e58788c875e9c354f359792cec016da0a1b0
Module : my_module
Function : sword_create
Arguments : [ImmOrOwnedObject((898922A9CABE93C6C38C55BBE047BFB0A8C864BF, SequenceNumber(1), o#9f12d4390e4fc8de3834c4960c6f265a78eca7c2b916ac1be66c1f00e1b47c68)), Pure([42, 0, 0, 0, 0, 0, 0, 0]), Pure([7, 0, 0, 0, 0, 0, 0, 0]), Pure([45, 50, 237, 113, 56, 27, 239, 127, 61, 140, 87, 180, 141, 248, 33, 35, 89, 54, 114, 170])]
Type Arguments : []

----- Transaction Effects ----
Status : Success { gas_cost: GasCostSummary { computation_cost: 69, storage_cost: 40, storage_rebate: 27 } }
Created Objects:
  - ID: 2E34983D59E9FC5310CFBAA953D2188E6A84FD21 , Owner: Account Address ( 2D32ED71381BEF7F3D8C57B48DF82123593672AA )
Mutated Objects:
  - ID: 58C4DAA98694266F4DF47BA436CD99659B6A5342 , Owner: Account Address ( ADE6EAD34629411F730416D6AD48F6B382BBC6FD )
  - ID: 898922A9CABE93C6C38C55BBE047BFB0A8C864BF , Owner: Account Address ( ADE6EAD34629411F730416D6AD48F6B382BBC6FD )
```

Go to the Sui Explorer to observe a newly created object. You should see a sword object created with `Magic` property of `42` and `Strength` property of `7` and transferred to the new owner.

Replace the object ID in the Explorer with the object ID of the created object you observed in your own command output, appended to:
https://explorer.sui.io/objects/

Related topics:
 * [Create Smart Contracts with Move](../build/move).
 * [Programming with Objects](../build/programming-with-objects/)
