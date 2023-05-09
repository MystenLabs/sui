---
title: Sui Intent Signing
---

In Sui, an intent is a compact struct that serves as the domain separator for a message that a signature commits to. The data that the signature commits to is an intent message. All signatures in Sui must commit to an intent message, instead of the message itself.

## Motivation

In previous releases, Sui used a special `Signable` trait that attached the Rust struct name as a prefix to the serialized data. This is not ideal because it's:
 * **Not compact:** The prefix `TransactionData::` is significantly larger than 1 byte.
 * **Not user-friendly:** Non-Rust applications need to maintain a list of Rust-struct names.

The intent signing standard provides a compact domain separator to the data being signed for both user signatures and authority signatures. It has several benefits, including:

 * The intent scope is replaced by an u8 representation instead of a Rust struct tag name string.
 * In addition to the intent scope, other important domain separators can be committed as well (e.g. intent version, app id).
 * The data itself no longer needs to implement the `Signable` trait, it just needs to implement `Serialize`.
 * All signatures can adopt the same intent message structure, including both user signatures (only to commit to `TransactionData`) and authority signature (commits to all internal intent scopes such as `TransactionEffects`, `ProofOfPossession`, and `SenderSignedTransaction`).

## Structs

The `IntentMessage` struct consists of the intent and the serialized data value.

```rust
pub struct IntentMessage<T> {
  pub intent: Intent,
  pub value: T,
}
```

To create an intent struct, include the `IntentScope` (what the type of the message is), `IntentVersion` (what version the network supports), and `AppId` (what application that the signature refers to).

```rust
pub struct Intent {
  scope: IntentScope,
  version: IntentVersion,
  app_id: AppId,
}
```

To see a detailed definition for each field, see each enum definition [in the source code](https://github.com/MystenLabs/sui/blob/0dc1a38f800fc2d8fabe11477fdef702058cf00d/crates/sui-types/src/intent.rs).

The serialization of an `Intent` is a 3-byte array where each field is represented by a byte.

The serialization of an `IntentMessage<T>` is the 3 bytes of the intent concatenated with the BCS serialized message.

## User Signature

To create a user signature, construct an intent message first, and create the signature over the 32-byte Blake2b hash of the BCS serialized value of the intent message of the transaction data (`intent || message`).

Here is an example in Rust:

```rust
let intent = Intent::default();
let intent_msg = IntentMessage::new(intent, data);
let signature = Signature::new_secure(&intent_msg, signer);
```

Here is an example in TypeScript:

```typescript
const intentMessage = messageWithIntent(
  IntentScope.TransactionData,
  transactionBytes,
);
const signature = await this.signData(intentMessage);
```

Under the hood, the `new_secure` method in Rust and the `signData` method in TypesScript does the following: 
 1. Serializes the intent message as the 3-byte intent concatenated with the BCS serialized bytes of the transaction data. 
 1. Applies Blake2b hash to get the 32-byte digest
 1. Passes the digest to the signing API for each corresponding scheme of the signer. The supported signature schemes are pure Ed25519, ECDSA Secp256k1 and ECDSA Secp256r1. See [Sui Signatures](sui-signatures.md#signature-requirements) for requirements of each scheme. 

## Authority Signature

The authority signature is created using the protocol key. The data that it commits to is also an intent message `intent || message`. See all available intent scopes [in the source code](https://github.com/MystenLabs/sui/blob/0dc1a38f800fc2d8fabe11477fdef702058cf00d/crates/sui-types/src/intent.rs#L66)

### How to Generate Proof of Possession for an Authority

When an authority request to join the network, the protocol public key and its proof of possession (PoP) are required to be submitted. PoP is required to prevent [rogue key attack](https://crypto.stanford.edu/~dabo/pubs/papers/BLSmultisig.html).

The proof of possession is a BLS signature created using the authority's protocol private key, committed over the following message: `intent || pubkey || address || epoch`. Here `intent` is serialized to `[5, 0, 0]` representing an intent with scope as "Proof of Possession", version as "V0" and app_id as "Sui". `pubkey` is the serialized public key bytes of the authority's BLS protocol key. `address` is the account address associated with the authority's account key. `epoch` is serialized to `[0, 0, 0, 0, 0, 0, 0, 0]`. 

To generate a proof of possession in Rust, see implementation at `fn generate_proof_of_possession`. For test vectors, see `fn test_proof_of_possession`. 

# Implementation

1. [Struct and enum definitions](https://github.com/MystenLabs/sui/blob/0dc1a38f800fc2d8fabe11477fdef702058cf00d/crates/sui-types/src/intent.rs)
2. [Test](https://github.com/MystenLabs/sui/blob/d009e82fa35bda4f2b3e7a86a9529d36c32a8159/crates/sui-types/src/unit_tests/intent_tests.rs)
3. User transaction intent signing [PR 1](https://github.com/MystenLabs/sui/pull/6445), [PR 2](https://github.com/MystenLabs/sui/pull/8321)
4. Authority intent signing [PR 1](https://github.com/MystenLabs/sui/pull/8154), [PR 2](https://github.com/MystenLabs/sui/pull/8726)