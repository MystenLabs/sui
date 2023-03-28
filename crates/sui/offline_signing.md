# Offline Signing

This is a guide for users who wish to sign a transaction using an offline device or implementing wallet in a different language without relying on sui keystore. Here are the steps to:
1. Serialize the data for signing;
2. Take the serialized data elsewhere to sign (wallet of your choice, or tools in other languages) to produce a signature and the corresponding public key.
3. Execute the signed transaction.

## Step 1: Serialize a Transfer

A transaction data must be serialized according to [BCS](https://crates.io/crates/bcs). It is supported in [other languages](https://github.com/zefchain/serde-reflection#language-interoperability).

Here an example is provided to serialize a transfer data in CLI. This outputs a serialized transaction data in Base64. The raw transaction to execute is what is submitted as tx_bytes.
 
```shell
target/debug/sui client serialize-transfer-sui --to $ADDRESS --sui-coin-object-id $OBJECT_ID --gas-budget 1000

Raw tx_bytes to execute: $TX_BYTES
```

## Step 2: Sign the data
This can be done elsewhere in your signing device or implemented in languages of your choice. Sui accepts signatures for pure Ed25519, ECDSA Secp256k1, ECDSA Secp256r1 and native multisig ([Read more](https://github.com/MystenLabs/sui/blob/d0aceaea613b33fc969f7ca2cdd84b8a35e87de3/crates/sui/multisig.md)).

The signature is committed to an intent message of the transaction data. See more on [intent message](https://github.com/MystenLabs/sui/blob/7f456d8e0db3234b178854df6185037e7b4312cb/crates/sui/intent_signing.md) on how to construct an intent message. 

Before passing in to the signing API, the intent message must be first hashed with Blake2b to 32-bytes. Depending on the signing scheme, an additional hashing is performed internally to the signing API. For ECDSA Secp256k1 and Secp256r1, SHA-2 SHA256 must be used as the internal hash function; for pure Ed25519, SHA-512 must be used. See below for additional signature requirements:

An accepted ECDSA Secp256k1 and Secp256r1 signature must follow:
1. The internal hash used by ECDSA must be SHA256 [SHA-2](https://en.wikipedia.org/wiki/SHA-2) hash of the transaction data. We use SHA256 because it is supported by [Apple](https://developer.apple.com/forums/thread/89619), HSMs, and [cloud](https://developer.apple.com/forums/thread/89619), and it is widely adopted by [Bitcoin](https://en.bitcoin.it/wiki/Elliptic_Curve_Digital_Signature_Algorithm).
2. The signature must be of length 64 bytes in the form of `[r, s]` where the first 32 bytes are `r`, the second 32 bytes are `s`.
3. The `r` value can be between 0x1 and 0xFFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFE BAAEDCE6 AF48A03B BFD25E8C D0364140 (inclusive).
4. The `s` value must be in the lower half of the curve order. If the signature is too high, please convert it to a lower `s` according to [BIP-0062](https://github.com/bitcoin/bips/blob/master/bip-0062.mediawiki#low-s-values-in-signatures) with the corresponding curve orders using `order - s`. For Secp256k1, the curve order is `0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141`. For Secp256r1, the curve order is `0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551` defined in [Standards for Efficient Cryptography](https://secg.org/SEC2-Ver-1.0.pdf).
5. Ideally, the signature must be generated with deterministic nonce according to [RFC6979](https://www.rfc-editor.org/rfc/rfc6979).

An accepted pure Ed25519 signature follows:
1. The signature must be produced according to [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032.html#section-5.1.6). The internal hash used is SHA-512.
2. The signature must be valid according to [ZIP215](https://github.com/zcash/zips/blob/main/zip-0215.rst).

Here we use the keytool command to sign as an example, using the Ed25519 key corresponding to the provided address stored in `sui.keystore`. This commands outputs the signature, the public key and the flag encoded in Base64. This command is backed by [fastcrypto](https://crates.io/crates/fastcrypto).
 
```shell
target/debug/sui keytool sign --address $ADDRESS --data $TX_BYTES

Signer address: $ADDRESS
Raw tx_bytes to execute: $TX_BYTES
Intent: Intent { scope: TransactionData, version: V0, app_id: Sui }
Raw intent message: $INTENT_MSG
Digest to sign: $DIGEST
Serialized signature (`flag || sig || pk` in Base64): $SERIALIZED_SIG
```

To ensure the signature produced offline matches with Sui's validity rules for testing purpose, you can import the mnemonics to sui.keystore using `sui keytool import` and then sign with `sui keytool sign` and compare the signature results. Additionally, test vectors can be found at `~/sui/sdk/typescript/test/e2e/raw-signer.test.ts`. 

To verify a signature against the cryptography library backing Sui when debugging, see [sigs-cli](https://github.com/MystenLabs/fastcrypto/blob/4cf71bd8b3a373495beeb77ce81c27827516c218/fastcrypto-cli/src/sigs_cli.rs).
## Step 3: Execute the signed transaction

Now that you have obtained the serialized signature, you can submit using the execution transaction command. This command takes `--tx-bytes` as the raw transaction bytes to execute (see output of `sui client serialize-transfer-sui`) and the serialized signature (Base64 encoded `flag || sig || pk`, see output of `sui keytool sign`). This executes the signed transaction and returns the certificate and transaction effects if successful.

```shell
sui client execute-signed-tx --tx-bytes $TX_BYTES --signatures $SERIALIZED_SIG
----- Certificate ----
Transaction Hash: $TX_ID
Transaction Signature: $SIGNATURE
Signed Authorities Bitmap: RoaringBitmap<[0, 1, 3]>
Transaction Kind : Transfer SUI
Recipient : $ADDRESS
Amount: Full Balance

----- Transaction Effects ----
Status : Success
Mutated Objects:
 - ID: $OBJECT_ID , Owner: Account Address ( $ADDRESS )
```