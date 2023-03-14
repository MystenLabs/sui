# Intent Signing


## Description

Intent Signing is a standard used in Sui that defines _what_ the signature is committed to. We call the data that the signature commits to is an intent message. All signatures in Sui MUST commit to an intent message, instead of the message itself.

## Motivation

Previously, Sui uses a special `Signable` trait, which attaches the Rust struct name as a prefix before the serialized data. This is not ideal because:
   - Not compact: The prefix of `TransactionData::` is significantly larger than 1 byte.
   - Not user-friendly: Non-Rust applications need to maintain a list of Rust-struct names.

The intent signing standard is proposed to provide a compact domain separator to the data being signed for both user signatures and authority signatures. It has several benefits:

1. The intent scope is replaced by an u8 representation instead of a Rust struct tag name string.

2. In addition to the intent scope, other important domain separators can be committed as well (e.g. intent version, app id).

3. The data itself no longer needs to implement `Signable` trait, but it just needs to implement the `Serialize`.

4. _All_ signatures can adopt the same intent message structure, including both user signatures (only to commit to `TransactionData`) and authority signature (commits to all internal intent scopes such as `TransactionEffects`, `ProofOfPossession`, `SenderSignedTransaction`, etc).

## Structs

The `IntentMessage` struct consists of the intent itself and the serialized data value.

```rust
pub struct IntentMessage<T> {
   pub intent: Intent,
   pub value: T,
}
```

An intent is a compact struct that serves as the domain separator for a message that a signature commits to. It consists of three parts: `IntentScope` (what the type of the message is), `IntentVersion` (what version the network supports), `AppId` (what application that the signature refers to).

```rust
pub struct Intent {
   scope: IntentScope,
   version: IntentVersion,
   app_id: AppId,
}
```

To see detailed definition for each field, see each enum definition [here](https://github.com/MystenLabs/sui/blob/0dc1a38f800fc2d8fabe11477fdef702058cf00d/crates/sui-types/src/intent.rs).


The serialization of an `Intent` is a 3-byte array where each field is represented by a byte.

The serialization of an `IntentMessage<T>` is the 3 bytes of the intent concatenated with the BCS serialized message.

## User Signature

To create a user signature, construct a intent message first, and create the signature over the BCS serialized value of the intent message (`intent || message`).

Here is an example in Rust:

```rust
let intent = Intent::default();
let intent_msg = IntentMessage::new(intent, data);
let signature = Signature::new_secure(&intent_msg, signer);
```

Here is an example in Typescript:

```typescript
const intentMessage = messageWithIntent(
   IntentScope.TransactionData,
   transactionBytes,
);
const signature = await this.signData(intentMessage);
```

## Authority Signature

Authority signature is created using the protocol key. The data that it commits to is also an intent message `intent || message`. See all available intent scopes [here](https://github.com/MystenLabs/sui/blob/0dc1a38f800fc2d8fabe11477fdef702058cf00d/crates/sui-types/src/intent.rs#L66)

# Implementation
1. [Struct and enum definitions](https://github.com/MystenLabs/sui/blob/0dc1a38f800fc2d8fabe11477fdef702058cf00d/crates/sui-types/src/intent.rs)
2. [Test](https://github.com/MystenLabs/sui/blob/d009e82fa35bda4f2b3e7a86a9529d36c32a8159/crates/sui-types/src/unit_tests/intent_tests.rs)
3. User transaction intent signing [PR 1](https://github.com/MystenLabs/sui/pull/6445), [PR 2](https://github.com/MystenLabs/sui/pull/8321)
4. Authority intent signing [PR 1](https://github.com/MystenLabs/sui/pull/8154), [PR 2](https://github.com/MystenLabs/sui/pull/8726)