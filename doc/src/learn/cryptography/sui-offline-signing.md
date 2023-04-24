---
title: Offline Signing
---

Sui supports offline signing, which is signing transactions using a device not connected to a Sui network, or in a wallet implemented in a different programming language without relying on the Sui key store. The steps to implement offline signing include:

 1. Serialize the data for signing.
 1. Sign the serialized data. Put the serialized data in a location to sign (such as the wallet of your choice, or tools in other programming languages) and to produce a signature with the corresponding public key.
 1. Execute the signed transaction.

## Serialize data for a transfer

You must serialize transaction data following [Binary Canonical Serialization](https://crates.io/crates/bcs) (BCS). It is supported in [other languages](https://github.com/zefchain/serde-reflection#language-interoperability).

The following example demonstrates how to to serialize data for a transfer using the Sui CLI. This returns serialized transaction data in Base64. Submit the raw transaction to execute as `tx_bytes`.
 
```shell
sui client serialize-transfer-sui --to $ADDRESS --sui-coin-object-id $OBJECT_ID --gas-budget 1000

Raw tx_bytes to execute: $TX_BYTES
```

## Sign the serialized data

You can sign the data using the device and programming language you choose. Sui accepts signatures for pure ed25519, ECDSA secp256k1, ECDSA secp256r1 and native multisig. To learn more about multisig, see [Sui Multi-Signature](sui-multisig.md).

The signature is committed to an intent message of the transaction data. To learn how to construct an intent message, see [Sui Intent signing](sui-intent_signing.md). 

Before you pass it in to the signing API, you must first hash the intent message to 32 bytes using Blake2b. To be compatible with existing standards and hardware secure modules (HSMs), the signing algorithms perform additional hashing internally. For ECDSA Secp256k1 and Secp256r1, you must use SHA-2 SHA256 as the internal hash function. For pure Ed25519, you must use SHA-512. 

Additional signature requirements:

An accepted ECDSA secp256k1 and secp256r1 signature must follow:
 1. The internal hash used by ECDSA must be SHA256 [SHA-2](https://en.wikipedia.org/wiki/SHA-2) hash of the transaction data. Sui uses SHA256 because it is supported by [Apple](https://developer.apple.com/forums/thread/89619), HSMs, and [cloud](https://developer.apple.com/forums/thread/89619), and it is widely adopted by [Bitcoin](https://en.bitcoin.it/wiki/Elliptic_Curve_Digital_Signature_Algorithm).
 1. The signature must be of length 64 bytes in the form of `[r, s]` where the first 32 bytes are `r`, the second 32 bytes are `s`.
 1. The `r` value can be between 0x1 and 0xFFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFE BAAEDCE6 AF48A03B BFD25E8C D0364140 (inclusive).
 1. The `s` value must be in the lower half of the curve order. If the signature is too high, convert it to a lower `s` according to [BIP-0062](https://github.com/bitcoin/bips/blob/master/bip-0062.mediawiki#low-s-values-in-signatures) with the corresponding curve orders using `order - s`. For secp256k1, the curve order is `0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141`. For secp256r1, the curve order is `0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551` defined in [Standards for Efficient Cryptography](https://secg.org/SEC2-Ver-1.0.pdf).
 1. Ideally, the signature must be generated with deterministic nonce according to [RFC6979](https://www.rfc-editor.org/rfc/rfc6979).

An accepted pure ed25519 signature follows:
 1. The signature must be produced according to [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032.html#section-5.1.6). The internal hash used is SHA-512.
 1. The signature must be valid according to [ZIP215](https://github.com/zcash/zips/blob/main/zip-0215.rst).

This example uses the `keytool` command to sign, using the ed25519 key corresponding to the provided address stored in `sui.keystore`. This commands outputs the signature, the public key, and the flag encoded in Base64. This command is backed by [fastcrypto](https://crates.io/crates/fastcrypto).
 
```shell
sui keytool sign --address $ADDRESS --data $TX_BYTES

Signer address: $ADDRESS
Raw tx_bytes to execute: $TX_BYTES
Intent: Intent { scope: TransactionData, version: V0, app_id: Sui }
Raw intent message: $INTENT_MSG
Digest to sign: $DIGEST
Serialized signature (`flag || sig || pk` in Base64): $SERIALIZED_SIG
```

To ensure the signature produced offline matches with Sui's validity rules for testing purposes, you can import the mnemonics to `sui.keystore` using `sui keytool import`. You can then sign with it using `sui keytool sign` and then compare the signature results. Additionally, you can find test vectors in `~/sui/sdk/typescript/test/e2e/raw-signer.test.ts`. 

To verify a signature against the cryptography library backing Sui when debugging, see [sigs-cli](https://github.com/MystenLabs/fastcrypto/blob/4cf71bd8b3a373495beeb77ce81c27827516c218/fastcrypto-cli/src/sigs_cli.rs).

## Execute the signed transaction

After you obtain the serialized signature, you can submit it using the execution transaction command. This command takes `--tx-bytes` as the raw transaction bytes to execute (see output of `sui client serialize-transfer-sui`) and the serialized signature (Base64 encoded `flag || sig || pk`, see output of `sui keytool sign`). This executes the signed transaction and returns the certificate and transaction effects if successful.

```shell
sui client execute-signed-tx --tx-bytes $TX_BYTES --signatures $SERIALIZED_SIG
----- Certificate ----
Transaction Hash: $TX_ID
Transaction Signature: $SIGNSTURE
Signed Authorities Bitmap: RoaringBitmap<[0, 1, 3]>
Transaction Kind : Transfer SUI
Recipient : $ADDRESS
Amount: Full Balance

----- Transaction Effects ----
Status : Success
Mutated Objects:
 - ID: $OBJECT_ID , Owner: Account Address ( $ADDRESS )
```