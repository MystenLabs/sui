---
title: Sui Signatures and Crypto Agility
---

We designed Sui with cryptographic agility in mind. The system supports multiple cryptography algorithms and primitives and can switch between them rapidly. Not only can developers choose best-of-breed cryptography for the system, they can implement the latest algorithms as they become available.

Sui defines its cryptography primitives, such as public key, signature, aggregated signature, and hash functions, under one unified type alias or enum wrapper that is shared across the entire repository. Making changes to these primitives affects all of an application's components. Developers can quickly update application cryptography and be assured of uniform security.

# User Signature

When a user submits a signed transaction, a serialized signature and a serialized transaction data is submitted. The serialized transaction data is the BCS serialized bytes of the struct `TransactionData` and the serialized signature is defined as a concatenation of bytes of `flag || sig || pk`.

The `flag` is a 1-byte representation corresponding to the signature scheme that the signer chooses. The signing scheme and its corresponding flag are defined below:

| Scheme | Flag |
|---|---|
|  Pure Ed25519 |  0x00 |
| ECDSA Secp256k1  | 0x01  |
| ECDSA Secp256r1  | 0x02  |
| MultiSig  | 0x03  |

The `sig` bytes is the compressed bytes representation of the signature instead of DER encoding. See the expected size and format below:

| Scheme | Signature |
|---|---|
|  Pure Ed25519 | Compressed, 64 bytes |
| ECDSA Secp256k1  | Compressed, 64 bytes  |
| ECDSA Secp256r1  | Compressed, 64 bytes  |
| MultiSig  | BCS serialized all signatures, size varies  |

The `pk` bytes is the bytes representation of the public key corresponding to the signature.

| Scheme | Public key |
|---|---|
|  Pure Ed25519 | Compressed, 32 bytes |
| ECDSA Secp256k1  | Compressed, 33 bytes  |
| ECDSA Secp256r1  | Compressed, 33 bytes  |
| MultiSig  | BCS serialized all participating public keys, size varies  |

## Signature Requirements

The signature must commit to the hash of the intent message of the transaction data, which can be constructed by appending the 3-byte intent before the BCS serialized transaction data. To learn more on what is an intent and how to construct an intent message, see [Sui Intent signing](sui-intent-signing.md).

When invoking the signing API, you must first hash the intent message of the transaction data to 32 bytes using Blake2b. This external hashing is distinct from the hashing performed inside the signing API. To be compatible with existing standards and hardware secure modules (HSMs), the signing algorithms perform additional hashing internally. For ECDSA Secp256k1 and Secp256r1, you must use SHA-2 SHA256 as the internal hash function. For pure Ed25519, you must use SHA-512.

An accepted ECDSA secp256k1 and secp256r1 signature must follow:

 1. The internal hash used by ECDSA must be SHA256 [SHA-2](https://en.wikipedia.org/wiki/SHA-2) hash of the transaction data. Sui uses SHA256 because it is supported by [Apple](https://developer.apple.com/forums/thread/89619), HSMs, and [cloud](https://developer.apple.com/forums/thread/89619), and it is widely adopted by [Bitcoin](https://en.bitcoin.it/wiki/Elliptic_Curve_Digital_Signature_Algorithm).
 1. The signature must be of length 64 bytes in the form of `[r, s]` where the first 32 bytes are `r`, the second 32 bytes are `s`.
 1. The `r` value can be between 0x1 and 0xFFFFFFFF FFFFFFFF FFFFFFFF FFFFFFFE BAAEDCE6 AF48A03B BFD25E8C D0364140 (inclusive).
 1. The `s` value must be in the lower half of the curve order. If the signature is too high, convert it to a lower `s` according to [BIP-0062](https://github.com/bitcoin/bips/blob/master/bip-0062.mediawiki#low-s-values-in-signatures) with the corresponding curve orders using `order - s`. For secp256k1, the curve order is `0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141`. For secp256r1, the curve order is `0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551` defined in [Standards for Efficient Cryptography](https://secg.org/SEC2-Ver-1.0.pdf).
 1. Ideally, the signature must be generated with deterministic nonce according to [RFC6979](https://www.rfc-editor.org/rfc/rfc6979).

An accepted pure Ed25519 signature must follow:
 1. The signature must be produced according to [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032.html#section-5.1.6). The internal hash used is SHA-512.
 1. The signature must be valid according to [ZIP215](https://github.com/zcash/zips/blob/main/zip-0215.rst).

See an concrete example for offline signing using CLI [here](../cryptography/sui-offline-signing.md).

# Authority Signature

The Authority on Sui (collection of validators) holds three distinctive keypairs:

 1. Protocol keypair
 1. Account keypair
 1. Network keypair

## Protocol Keypair
The Protocol keypair provides authority signatures on user-signed transactions if they are verified. When a stake of the authorities that provide signatures on user transactions passes the required two-thirds threshold, Sui executes the transaction. We currently chose the BLS12381 scheme for its fast verification on aggregated signatures for a given number of authorities. In particular, we decided to use the minSig BLS mode, according to which each individual public key is 96 bytes, while the signature is 48 bytes. The latter is important as typically validators register their keys once at the beginning of each epoch and then they continuously sign transactions; thus, we optimize on minimum signature size.

Note that with the BLS scheme, one can aggregate independent signatures resulting in a single BLS signature payload. We also accompany the aggregated signature with a bitmap to denote which of the validators signed. This effectively reduces the authorities' signature size from (2f + 1) × BLS_sig size to just one BLS_sig payload, which in turn has significant network cost benefits resulting in compressed transaction certificates independently on the validators set size.

To counter potential rogue key attacks on BLS12381 aggregated signatures, proof of knowledge of the secret key (KOSK) is used during authority registration. When an authority requests to be added to the validator set, a proof of possession is submitted and verified. See [here](sui-intent-signing.md) on how to create a Proof of Possession. Unlike most standards, our proof of knowledge scheme commits to the address as well, which offers an extra protection against adversarial reuse of a validator;s BLS key from another malicious validator.

## Account Keypair
The account that the authority uses to receive payments on staking rewards is secured by the account keypair. We use pure Ed25519 as the signing scheme.

## Network Keypair
The private key is used to perform the TLS handshake required by QUIC for Narwhal primary and its worker network interface. The public key is used for validator peer ID. Pure Ed25519 is used as the signing scheme.

See more authority key toolings in [Validator Tool](../../../../nre/validator_tool.md), 