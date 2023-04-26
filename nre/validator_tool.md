# Validator Tool

## Overview

This document is focused on using Validator Tool.

**Caveat: this tool only supports Pending Validators and Active Validators at the moment.**

## Preparation

1. Make sure you have completed all the [prerequisites](https://docs.sui.io/devnet/build/install).

2. Build the `sui` binary, which you will need for the genesis ceremony. This step can be done on any machine you like. It does not have to be done on the machine on which you will run the validator.

    1. Clone the git repo:

           git clone git@github.com:MystenLabs/sui.git && cd sui

    2. Check out the commit we will be using for the testnet:

           git checkout testnet

    3. Build sui binary

           cargo build --bin sui

    4. Remember the path to your binary:

           export SUI_BINARY="$(pwd)/target/debug/sui"

3. Run the following command to set up your Sui account and CLI environment. 

    1. If this is the first time running this program, it will ask you to provide a Sui Fullnode Server URL and a meaningful environment alias. It will also generate a random key pair in `sui.keystore` and a config `client.yaml`. Swap in your validator account key if you already have one.

    2. If you already set it up, simply make sure 
      a. `rpc` is correct in `client.yaml`. 
      b. `active_address` is correct in `client.yaml`.
      b. `sui.keystore` contains your account key pair.

    If at this point you can't find where `client.yaml` or `sui.keystore` is or have other questions, read [Sui Client CLI tutorial](https://docs.sui.io/devnet/build/cli-client).

``` bash
$SUI_BINARY client
```

4. To test you are connected to the network and configured your config correctly, run the following command to display your validator info.

``` bash
$SUI_BINARY validator display-metadata
```



## Using Validator Tool

#### Print Help Info
``` bash
$SUI_BINARY validator --help
```

#### Display Validator Metadata
``` bash
$SUI_BINARY validator display-metadata
```

or 

``` bash
$SUI_BINARY validator display-metadata <validator-address>
```
to print another validator's information.

#### Update Validator Metadata
Run the following to see how to update validator metadata. Read description carefully about when the change will take effect.
``` bash
$SUI_BINARY validator update-metadata --help
```

You can update the following on-chain metadata:
1. name
2. description
3. image URL
4. project URL
5. network address
6. p2p address
7. primary address
8. worker address
9. protocol public key
10. network public key
11. worker public key

Notably, only the first 4 metadata listed above take effect immediately.

If you change any metadata from points 5 to 11, they will be changed only after the next epoch - **for these, you'll want to restart the validator program immediately after the next epoch, with the new key files and/or updated `validator.yaml` config. Particularly, make sure the new address is not behind a firewall.**

Run the following to see how to update each metadata.
``` bash
$SUI_BINARY validator update-metadata --help
```

#### Operation Cap
Operation Cap allows a validator to authorizer another account to perform certain actions on behalf of this validator. Read about [Operation Cap here](sui_for_node_operators.md#operation-cap).

The Operation Cap holder (either the valdiator itself or the delegatee) updates its Gas Price and reports validator peers with the Operation Cap.

#### Update Gas Price
To update Gas Price, run

```bash
$SUI_BINARY validator update-gas-price <gas-price>
```

if the account itself is a validator and holds the Operation Cap. Or 

```bash
$SUI_BINARY validator update-gas-price --operation-cap-id <operation-cap-id> <gas-price>
```

if the account is a delegatee.

#### Report Validators
To report validators peers, run

```bash
$SUI_BINARY validator report-validator <reportee-address>
```

Add `--undo-report false` if it intents to undo an existing report.

Similarly, if the account is a delegatee, add `--operation-cap-id <operation-cap-id>` option to the command.

if the account itself is a validator and holds the Operation Cap. Or 

```bash
$SUI_BINARY validator update-gas-price --operation-cap-id <operation-cap-id> <gas-price>
```

if the account is a delegatee.


#### Become a Validator / Join Committee
To become a validator candidate, first run

```bash
$SUI_BINARY validator make-validator-info <name> <description> <image-url> <project-url> <host-name> <gas_price>
```

This will generate a `validator.info` file and key pair files. The output of this command includes:
  1. Four key pair files (Read [more here](sui_for_node_operators.md#key-management)). ==Set their permissions with the minimal visibility (chmod 600, for example) and store them securely==. They are needed when running the validator node as covered below.
    a. If you follow this guide thoroughly, this key pair is actually copied from your `sui.keystore` file.
  2. `validator.info` file that contains your validator info. **Double check all information is correct**.

Then run 

``` bash
$SUI_BINARY validator become-candidate {path-to}validator.info
```

to submit an on-chain transaction to become a validator candidate. The parameter is the file path to the validator.info generated in the previous step. **Make sure the transaction succeeded (printed in the output).**

At this point you are validator candidate and can start to accept self staking and delegated staking. 

**If you haven't, start a fullnode now to catch up with the network. When you officially join the committee but is not fully up-to-date, you cannot make meaningful contribution to the network and may be subject to peer reporting hence face the risk of reduced staking rewards for you and your delegators.**

Once you collect enough staking amount, run

``` bash
$SUI_BINARY validator join-committee
```

to become a pending validator. A pending validator will become active and join the committee starting from next epoch.


#### Leave Committee

To leave committee, run

``` bash
$SUI_BINARY validator leave-committee
```

Then you will be removed from committee starting from next epoch.

### Generate the payload to create PoP

Serialize the payload that is used to generate Proof of Possession. This is allows the signer to take the payload offline for an Authority protocol BLS keypair to sign.

``` bash
$SUI_BINARY validator serialize-payload-pop --account-address $ACCOUNT_ADDRESS --protocol-public-key $BLS_PUBKEY
Serialized payload: $PAYLOAD_TO_SIGN
```