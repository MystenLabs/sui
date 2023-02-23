# Multisig

Here we demonstrate the workflow to create a multisig transaction and submits to Sui using CLI against Localnet. 

## Step 1: Add some keys to keystore

Here we use the sui.keystore to demonstrate. 

```shell
target/debug/sui client new-address ed25519
target/debug/sui client new-address secp256k1
target/debug/sui client new-address secp256r1

target/debug/sui keytool list 

Sui Address | Public Key (Base64) | Scheme
--------------------------------------------------------------------------
 $ADDR_1    | $PK_1               | secp256r1
 $ADDR_2    | $PK_2               | secp256k1
 $ADDR_3    | $PK_3               | ed25519
```
## Step 2: Create a multisig address

Input a list of public keys to use for the multisig address, and a list of their corresponding weights.

```
target/debug/sui keytool multi-sig-address --pks $PK_1 $PK_2 $PK_3 --weights 1 2 3 --threshold 3
Multisig address: $MULTISIG_ADDR

Participating parties:
Sui Address | Public Key (Base64)| Weight
------------------------------------------
 $ADDR_1    | $PK_1              |   1
 $ADDR_2    | $PK_2              |   2
 $ADDR_3    | $PK_3              |   3
```
## Step 3: Send some objects to this multisig address

Here we assume requesting gas from Localnet. The url uses default faucet from running `cargo run --bin sui-test-validator`.

```
curl --location --request POST 'http://127.0.0.1:9123/gas' --header 'Content-Type: application/json' --data-raw "{ \"FixedAmountRequest\": { \"recipient\": \"$MULTISIG_ADDR\" } }"

{"transferred_gas_objects":[{"amount":200000,"id":"$OBJECT_ID", ...}]}
```

## Step 3: Serialize a transaction

For example, we use an object that belongs to the multisig address and serialize a transfer to be signed. This can be any serialized transaction data where the sender is the multisig address. 

```
target/debug/sui client serialize-transfer-sui --to 0x183ee5473ffecfc959d0c547a6198b94e3c2c971 --sui-coin-object-id $OBJECT_ID --gas-budget 1000

Raw tx_bytes to execute: $TX_BYTES
```

## Step 4: Sign the transaction with two keys

Here we demonstrate how to sign with two keys in sui.keystore. This can be done with other tools as long as it serializes to `flag || sig || pk`. 

```
target/debug/sui keytool sign --address $ADDR_1 --data $TX_BYTES

Raw tx_bytes to execute: $TX_BYTES
Serialized signature (`flag || sig || pk` in Base64): $SIG_1

target/debug/sui keytool sign --address $ADDR_2 --data $TX_BYTES

Raw tx_bytes to execute: $TX_BYTES
Serialized signature (`flag || sig || pk` in Base64): $SIG_2
```

## Step 5: Combine individual signatures into a multisig

```
target/debug/sui keytool multi-sig-combine-partial-sig --pks $PK_1 $PK_2 $PK_3 --weights 1 2 3 --threshold 3 --sigs $SIG_1 $SIG_2
```

## Step 6: Execute a transaction with multisig

```
target/debug/sui client execute-signed-tx --tx-bytes $TX_BYTES --signature $SERIALIZED_MULTISIG
```