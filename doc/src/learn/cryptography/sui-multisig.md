---
title: Sui Multi-Signature
---

Sui supports `k` out of `n` multi-weight multi-scheme multi-signature (multisig) transactions where `n <= 10`. 
A multisig transaction is one that requires more than one private key to authorize it. This topic demonstrates the 
workflow to create a multisig transaction in Sui, and then submit it using the Sui CLI against a local network. To learn
how to set up a local network, see [Sui Local Network](../build/sui-local-network.md).

## Applications of multi-signatures

Multi-signature wallets offer several unique benefits to users, from increased security to escrow transactions and 2FA. 
Unlike conventional multisig wallets, a Sui multisig account can accept weighted public keys from multiple key schemes.
Currently, each key can be one of the following schemes: Ed25519, ECDSA secp256k1, and ECDSA secp256r1. 

Interestingly, cryptographic agility allows users to mix and match key schemes in a single multisig account. For 
example, one can pick a single Ed25519 mnemonic-based key and two ECDSA secp256r1 key to create a multisig account that 
always requires the Ed25519 key, but also one of the ECDSA secp256r1 keys to sign. A potential application of the above
structure is using mobile secure enclave stored keys as 2FA; note that currently iPhone and high-end Android devices 
support ECDSA secp256r1 enclave-stored keys only.

![Sui tokenomics flow](../../../static/cryptography/sui_multisig_structures.png "Multisig Sui supported structures")
*Examples of Sui supported multisig structures.*

## Step 1: Add keys to Sui keystore

Use the following command to generate a Sui address and key for each supported key scheme and add it to the `sui.keystore`, then list the keys.

```shell
sui client new-address ed25519
sui client new-address secp256k1
sui client new-address secp256r1

sui keytool list
```

The response resembles the following, but displays actual addresses and keys:

```
Sui Address | Public Key (Base64) | Scheme
--------------------------------------------------------------------------
$ADDR_1     | $PK_1               | secp256r1
$ADDR_2     | $PK_2               | secp256k1
$ADDR_3     | $PK_3               | ed25519
```

## Step 2: Create a multisig address

To create a multisig address, input a list of public keys to use for the multisig address and list their corresponding weights.

```shell
sui keytool multi-sig-address --pks $PK_1 $PK_2 $PK_3 --weights 1 2 3 --threshold 3
Multisig address: $MULTISIG_ADDR
```

The response resembles the following:

```
Participating parties:
Sui Address | Public Key (Base64)| Weight
------------------------------------------
$ADDR_1    | $PK_1              |   1
$ADDR_2    | $PK_2              |   2
$ADDR_3    | $PK_3              |   3
```

## Step 3: Send objects to a multisig address

This example requests gas from a local network using the default URL following the guidance in [Sui Local Network](../build/sui-local-network.md).


```shell
curl --location --request POST 'http://127.0.0.1:9123/gas' --header 'Content-Type: application/json' --data-raw "{ \"FixedAmountRequest\": { \"recipient\": \"$MULTISIG_ADDR\" } }"
```

The response resembles the following:
```
{"transferred_gas_objects":[{"amount":200000,"id":"$OBJECT_ID", ...}]}
```

## Step 3: Serialize a transaction

This section demonstrates how to use an object that belongs to a multisig address and serialize a transfer to be signed. This can be any serialized transaction data where the sender is the multisig address.

```shell
sui client serialize-transfer-sui --to $$MULTISIG_ADDR --sui-coin-object-id $OBJECT_ID --gas-budget 1000

Raw tx_bytes to execute: $TX_BYTES
```

## Step 4: Sign the transaction with two keys

Use the following code sample to sign the transaction with two keys in `sui.keystore`. You can do this with other tools as long as you serialize it to `flag || sig || pk`.

```shell
sui keytool sign --address $ADDR_1 --data $TX_BYTES

Raw tx_bytes to execute: $TX_BYTES
Serialized signature (`flag || sig || pk` in Base64): $SIG_1

sui keytool sign --address $ADDR_2 --data $TX_BYTES

Raw tx_bytes to execute: $TX_BYTES
Serialized signature (`flag || sig || pk` in Base64): $SIG_2
```

## Step 5: Combine individual signatures into a multisig

This sample demonstrates how to combine the two signatures:
```shell
sui keytool multi-sig-combine-partial-sig --pks $PK_1 $PK_2 $PK_3 --weights 1 2 3 --threshold 3 --sigs $SIG_1 $SIG_2
```

## Step 6: Execute a transaction with multisig

This sample demonstrates how to execute a transaction using multisig:
```shell
sui client execute-signed-tx --tx-bytes $TX_BYTES --signature $SERIALIZED_MULTISIG
```
