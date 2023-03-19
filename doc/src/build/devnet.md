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
Config file ["/Users/dir/.sui/sui_config/client.yaml"] doesn't exist, do you want to connect to a Sui Full node server [y/N]?
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

Sui returns a message similar to the following (depending on the key scheme you selected) that includes the address and 24-word recovery phrase for the address:

```
Generated new keypair for address with scheme "ed25519" [0xb9c83a8b40d3263c9ba40d551514fbac1f8c12e98a4005a0dac072d3549c2442]
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
  !faucet `<Your client address>`

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
INCLUDING DEPENDENCY Sui
INCLUDING DEPENDENCY MoveStdlib
BUILDING MyFirstPackage
Successfully verified dependencies on-chain against source.
----- Transaction Data ----
Transaction Signature: [Signature(Ed25519SuiSignature(Ed25519SuiSignature([0, 125, 78, 25, 52, 44, 203, 148, 143, 172, 110, 105, 88, 189, 236, 21, 138, 189, 112, 187, 194, 114, 84, 187, 38, 11, 110, 190, 132, 156, 55, 133, 83, 217, 107, 116, 84, 218, 245, 76, 35, 142, 168, 225, 28, 203, 249, 10, 215, 121, 50, 75, 57, 182, 174, 91, 9, 101, 132, 93, 135, 17, 130, 185, 1, 179, 217, 77, 101, 114, 102, 91, 188, 47, 119, 202, 180, 98, 71, 241, 49, 221, 180, 213, 109, 5, 155, 216, 106, 151, 199, 153, 67, 200, 193, 58, 158])))]
Transaction Kind : Programmable
Inputs: ["0xf505046be474fd6ee5cdf7bb16fba9618d9c7fe040d494ab1af1c143e43d1e3f"]
Commands: [
  Publish(_),
  TransferObjects([Result(0)],Input(0)),
]

Sender: 0xf505046be474fd6ee5cdf7bb16fba9618d9c7fe040d494ab1af1c143e43d1e3f
Gas Payment: Object ID: 0x53ee9fde15ff86e49f6af62c435266c8282c2d7cb7b1586c28f8c1c0e50d606a, version: 0x1, digest: HJws2g8eikAhnik4WkpMPUPxcbBBPwtGJCXmQbVjFfya 
Gas Owner: 0xf505046be474fd6ee5cdf7bb16fba9618d9c7fe040d494ab1af1c143e43d1e3f
Gas Price: 1
Gas Budget: 30000
----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: 0x0afb60ea71302446ba4bf3a1e7b0c20a644db634fd2f0e32aa4ca9354dcfa4f8 , Owner: Immutable
  - ID: 0x76c0ad2883daba4707184b34389abc8296f864bfc2efb12f13f7022a477d4a6f , Owner: Account Address ( 0xf505046be474fd6ee5cdf7bb16fba9618d9c7fe040d494ab1af1c143e43d1e3f )
  - ID: 0x7883028e68250215bcec8856f4fbce1e5edb813e58115230549768288dd1dcc9 , Owner: Account Address ( 0xf505046be474fd6ee5cdf7bb16fba9618d9c7fe040d494ab1af1c143e43d1e3f )
Mutated Objects:
  - ID: 0x53ee9fde15ff86e49f6af62c435266c8282c2d7cb7b1586c28f8c1c0e50d606a , Owner: Account Address ( 0xf505046be474fd6ee5cdf7bb16fba9618d9c7fe040d494ab1af1c143e43d1e3f )
```

The package publish operation creates several objects:

* A package object (with ID `0x0afb60ea71302446ba4bf3a1e7b0c20a644db634fd2f0e32aa4ca9354dcfa4f8` in the example output).
* A `Forge` object (with ID `0x76c0ad2883daba4707184b34389abc8296f864bfc2efb12f13f7022a477d4a6f` in the example output) as a result of running a [module initializer](../build/move/debug-publish.md#module-initializers) for the one (and only) module of this package.
* An `UpgradeCap` object (with ID `0x7883028e68250215bcec8856f4fbce1e5edb813e58115230549768288dd1dcc9` in the example output) for use in future package upgrades.

You can check the details of each object using the `sui client object <OBJECT_ID>` command or by using the Sui Explorer.

When you publish a package, the IDs for the objects the compiler creates are different than the ones displayed in this example. The remainder of this topic uses `<PACKAGE_ID>` and `<FORGE_ID>` to represent the IDs for the created objects (except for console output examples). You must replace these with the values for your package.

## Make a Move call

This section describes how to call into functions defined in the package published in the previous section. Use the (`<PACKAGE_ID>`) and (`<FORGE_ID>`) values from your package to create swords and transfer them to other players.

To demonstrate this, `<PLAYER_ADDRESS>` represents the address of the player to receive a sword. You can use an address of someone you know, or create another address for testing with the following Sui Client CLI command:
```shell
sui client new-address ed25519
```

The command returns the following message and a 24-word recovery phrase for the address:
```shell
Created new keypair for address with scheme ED25519: [0xa01cd0c520f12a1e9d57bf3cc6ea0f8cf93e81e9fe46f7b4916c310a809dfddd]
Secret Recovery Phrase : [sunny tip element salad frequent february amount notice chair kite race push noise ketchup that same cannon bench mirror please dinosaur indicate violin sunset]
```

To create a sword and transfer it to another player, we use the following command to call the `sword_create` [function](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/move_tutorial/sources/my_module.move#L47) in the `my_module` [module](https://github.com/MystenLabs/sui/blob/main/sui_programmability/examples/move_tutorial/sources/my_module.move#L4) of the package we previously published.

You must use the same format for the command and function parameters as the example shown.

```shell
sui client call --function sword_create --module my_module --package 0x<PACKAGE_ID> --args \"0x<FORGE_ID>\" 42 7 \"0x<PLAYER_ADDRESS>\" --gas-budget 30000
```

The response resembles the following:
```shell
----- Transaction Data ----
Transaction Signature: [Signature(Ed25519SuiSignature(Ed25519SuiSignature([0, 178, 50, 21, 4, 228, 131, 133, 216, 36, 226, 72, 149, 116, 225, 128, 3, 228, 203, 17, 185, 167, 107, 91, 123, 253, 21, 24, 10, 91, 152, 215, 199, 215, 144, 183, 170, 21, 66, 83, 27, 161, 252, 224, 42, 52, 97, 242, 186, 35, 32, 2, 222, 97, 167, 67, 197, 244, 60, 114, 32, 70, 41, 35, 2, 179, 217, 77, 101, 114, 102, 91, 188, 47, 119, 202, 180, 98, 71, 241, 49, 221, 180, 213, 109, 5, 155, 216, 106, 151, 199, 153, 67, 200, 193, 58, 158])))]
Transaction Kind : Programmable
Inputs: ["76c0ad2883daba4707184b34389abc8296f864bfc2efb12f13f7022a477d4a6f", [42,0,0,0,0,0,0,0], "\u0000\u0000\u0000\u0000\u0000\u0000\u0000", "0xa01cd0c520f12a1e9d57bf3cc6ea0f8cf93e81e9fe46f7b4916c310a809dfddd"]
Commands: [
  MoveCall(0x0afb60ea71302446ba4bf3a1e7b0c20a644db634fd2f0e32aa4ca9354dcfa4f8::my_module::sword_create(,Input(0),Input(1),Input(2)Input(3))),
]

Sender: 0xf505046be474fd6ee5cdf7bb16fba9618d9c7fe040d494ab1af1c143e43d1e3f
Gas Payment: Object ID: 0x53ee9fde15ff86e49f6af62c435266c8282c2d7cb7b1586c28f8c1c0e50d606a, version: 0x2, digest: 7SchKLPQhp18WwTSJHqvnb6cddzcBPVokn1erYAiiU6r 
Gas Owner: 0xf505046be474fd6ee5cdf7bb16fba9618d9c7fe040d494ab1af1c143e43d1e3f
Gas Price: 1
Gas Budget: 30000
----- Transaction Effects ----
Status : Success
Created Objects:
  - ID: 0x3265ebb34fc92ab905ec312a03044c03afaa4375dc83c44dee3c5246a8b67163 , Owner: Account Address ( 0xa01cd0c520f12a1e9d57bf3cc6ea0f8cf93e81e9fe46f7b4916c310a809dfddd )
Mutated Objects:
  - ID: 0x53ee9fde15ff86e49f6af62c435266c8282c2d7cb7b1586c28f8c1c0e50d606a , Owner: Account Address ( 0xf505046be474fd6ee5cdf7bb16fba9618d9c7fe040d494ab1af1c143e43d1e3f )
  - ID: 0x76c0ad2883daba4707184b34389abc8296f864bfc2efb12f13f7022a477d4a6f , Owner: Account Address ( 0xf505046be474fd6ee5cdf7bb16fba9618d9c7fe040d494ab1af1c143e43d1e3f )
```

Go to the Sui Explorer to observe a newly created object. You should see a sword object created with `Magic` property of `42` and `Strength` property of `7` and transferred to the new owner.

![Object view in Sui Explorer](../../static/build-explorer-object.png)
*Explorer view of example sword object*

To see your object in the current [Sui Explorer](https://explorer.sui.io), paste the object ID of the created object you observed in your own command output in the search field and press **Enter**.

Related topics:
 * [Create Smart Contracts with Move](../build/move/index.md)
 * [Programming with Objects](../build/programming-with-objects/index.md)
