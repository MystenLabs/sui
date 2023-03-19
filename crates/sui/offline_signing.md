# Offline Signing

This is a guide for users who wish to sign a transaction using an offline device without relying on sui keystore. Here are the steps to:
1. Serialize the data for signing;
2. Take the serialized data elsewhere to sign (wallet of your choice, or tools in other languages) to produce a signature and the corresponding public key.
3. Execute the signed transaction.

## Step 1: Serialize a Transfer

A transaction data must be serialized according to [BCS](https://crates.io/crates/bcs). It is supported in [other languages](https://github.com/zefchain/serde-reflection#language-interoperability).

Here an example is provided to serialize a transfer data in CLI. This outputs a serialized transaction data in Base64. The intent message to sign is what the signature commits to. The raw transaction to execute is what is submitted as tx_bytes.
 
```shell
target/debug/sui client serialize-transfer-sui --to 0xfdf3a56d8ac390499c611fd338036e3139a0e9a5 --sui-coin-object-id 0x14808dbfbb3efd6fa09624fd18d7f40958679fa1 --gas-budget 1000

Raw tx_bytes to execute: $TX_BYTES
```

## Step 2: Sign the data
This can be done elsewhere in your signing device or implemented in languages of your choice. Sui accepts signatures for pure Ed25519, ECDSA Secp256k1, ECDSA Secp256r1 and native multisig ([Read more](https://github.com/MystenLabs/sui/blob/d0aceaea613b33fc969f7ca2cdd84b8a35e87de3/crates/sui/multisig.md)).

An accepted ECDSA Secp256k1 andd Secp256r1 signature follows:
1. The signature must be committed to the SHA256 [SHA-2](https://en.wikipedia.org/wiki/SHA-2) hash of the transaction data. We use SHA256 because it is supported by [Apple](https://developer.apple.com/forums/thread/89619), HSMs, and [cloud](https://developer.apple.com/forums/thread/89619), and it is widely adopted by [Bitcoin](https://en.bitcoin.it/wiki/Elliptic_Curve_Digital_Signature_Algorithm).
2. The signature must be of length 64 bytes in the form of `[r, s]` where the first 32 bytes are `r`, the second 32 bytes are `s`.
3. The `r` value can be between 0x1 and 0xFFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFE BAAEDCE6 AF48A03B BFD25E8C D0364140 (inclusive).
4. The `s` value must be in the lower half of the curve order, i.e. between 0x1 and 0x7FFFFFFF FFFFFFFF FFFFFFFF FFFFFFFF 5D576E73 57A4501D DFE92F46 681B20A0 (inclusive). If the signature is not guranteed to have `s` in the lower range by the signing function, please convert it to a lower `s` according to [this guide](https://github.com/bitcoin/bips/blob/master/bip-0062.mediawiki#low-s-values-in-signatures).
5. Ideally, the signature must be generated with deterministic nonce according to [RFC6979](https://www.rfc-editor.org/rfc/rfc6979).

An accepted pure Ed25519 signature follows:
1. The signature must be produced according to [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032.html#section-5.1.6).
2. The signature must be valid according to [ZIP215](https://github.com/zcash/zips/blob/main/zip-0215.rst).

Here we use the keytool command to sign as an example, using the Ed25519 key corresponding to the provided address stored in `sui.keystore`. This commands outputs the signature, the public key and the flag encoded in Base64. This command is backed by [fastcrypto](https://crates.io/crates/fastcrypto).
 
```shell
target/debug/sui keytool sign --address 0xb59ce11ef3ad15b6c247dda9890dce1b781f99df --data $TX_BYTES

Signer address: 0xa5f022cce499749a54ba59bf377cdaea369e7457
Raw tx_bytes to execute: $TX_BYTES
Intent: Intent { scope: TransactionData, version: V0, app_id: Sui }
Intent message to sign: "AAAAAAMYPuVHP/7PyVnQxUemGYuU48LJcQBnglR8A4kz7KFUQOOUum3/eljiCBUxwn6yBVIxn5CDBNkN0CJf0fI/AgAAAAAAAAAgytK3HSb0y/qcT5VC23nt187atosf3Te8NEJ2tNGQ11sBAAAAAAAAABAnAAAAAAAA"
Serialized signature (`flag || sig || pk` in Base64): $SERIALIZED_SIG
```

To verify a signature against the cryptography library backing Sui when debugging, see [sigs-cli](https://github.com/MystenLabs/fastcrypto/blob/4cf71bd8b3a373495beeb77ce81c27827516c218/fastcrypto-cli/src/sigs_cli.rs).

## Step 3: Execute the signed transaction

Now that you have obtained the serialized signature, you can submit using the execution transaction command. This command takes `--tx-bytes` as the raw transaction bytes to execute (see output of `sui client serialize-transfer-sui`) and the serialized signature (Base64 encoded `flag || sig || pk`, see output of `sui keytool sign`). This executes the signed transaction and returns the certificate and transaction effects if successful.

```shell
sui client execute-signed-tx --tx-bytes $TX_BYTES --signatures $SERIALIZED_SIG
----- Certificate ----
Transaction Hash: wnk9u71q8mhPgEOrDZJacVyqAzNBAmsMOPM4rNoS0LE=
Transaction Signature: AA==@epIttAjg4OBOzVBQQuMflR9sJwh12XiBFwDV9gmiBxomKJ0YyjcbhLONdvA1xs2NXy8xdagwHR/uRVdI6z+LAg==@rJzjxQ+FCK9m8YDU8Dq1Yx931HkIArhcw33kUPL9P8c=
Signed Authorities Bitmap: RoaringBitmap<[0, 1, 3]>
Transaction Kind : Transfer SUI
Recipient : 0x581a119a6576d3b502b5dc47c5de497b774e68ca
Amount: Full Balance

----- Transaction Effects ----
Status : Success
Mutated Objects:
 - ID: 0x0599b794da39169f7c75d34eba06ae105fedc61b , Owner: Account Address ( 0x581a119a6576d3b502b5dc47c5de497b774e68ca )
```