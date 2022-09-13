# Developer Experience Roadmap
(last updated 9/9/2022, next update approximately ~10/9/2022 following a monthly cadence)

To keep Sui builders up to date with the latest happenings, we are maintaining the following list of developer-facing changes coming in the next ~30 days. While we strive to be accurate, the timing of the landing/release of these features is subject to change. More thorough documentation and references will be available as each feature is released to Devnet. Please continue to monitor Devnet release notes for the source of truth on what is currently deployed--this list is about what's next!

## Big picture
* The gateway component will be deprecated. Clients will now communicate directly with full nodes, and full nodes will act as quorum driver (i.e., accept transactions, form a certificate + certificate effects, return the effects to the client)
* Child objects are getting a big revamp: they no longer need to be passed as explicit transaction inputs, and rich new collection types (e.g., large hash maps with dynamic lookup) will be possible. More details in the Sui Move section.

## JSON RPC
* [Gateway deprecation] `sui_syncAccountState` will be deprecated and removed.
* [Gateway deprecation] Gateway’s `sui_executeTransaction` method will be replaced by fullnode’s `sui_executeTransaction` method which has an extra parameter that allows users to choose whether a method should block or return immediately.
* Pagination support for the following APIs.
  * `get_objects_owned_by_address`
  * `get_objects_owned_by_object`
  * `get_recent_transactions`
  * `get_transactions_by_input_object`
  * `get_transactions_by_mutated_object`
  * `get_transactions_by_move_function`
  * `get_transactions_from_addr`
  * `get_transactions_to_addr`
  * `get_events_by_transaction`
  * `get_events_by_transaction_module`
  * `get_events_by_move_event_struct_name`
  * `get_events_by_sender`
  * `get_events_by_recipient`
  * `get_events_by_object`
  * `get_events_by_timerange`
* A `sui_call` API that emulates transaction execution, but does not commit the transaction to the network, akin to eth_call in Ethereum. This is useful for gas cost prediction and transaction effect simulation.
* `sui_eventSubscription` API that enables subscribing to all transaction events, including ones that don’t generate network activity ([issue](https://github.com/MystenLabs/sui/pull/4288))

## SDK (Typescript, Rust)
* Reliability improvements in Typescript SDK support for `sui_publish` API
* Support for `sui_subscribeEvent` in the Typescript SDK
* Rust SDK will be kept at parity with the Typescript SDK when feasible

# Sui Move
* Support for dynamic access of child objects to avoid explicitly passing child objects as transaction inputs and to enable implementation of design patterns/collections that require pointer-like parent->child representation. ([issue](https://github.com/MystenLabs/sui/issues/4203))
* Support for passing strings and object ID encodings to entry functions
* This frequently requested feature allows passing a vector of objects to entry functions which will open up a variety of flexible use cases *wrt* payments and coin management.
* A collection of cryptographic libraries are exposed as Move native functions to use on-chain. They can be found at:
  * [crypto.move](https://github.com/MystenLabs/sui/blob/4ed29873e35cad9d709dbd9370e17d261446e873/crates/sui-framework/sources/crypto/elliptic_curve.move): This contains various utility methods for on-chain signature verification. It also provides a method to recover Secp256k1 signature to its public key, bulletproof verification, and Pedersen commitments. See this work in action in the Cryptography (math) [example](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/math).
  * [elliptic_curve.move](https://github.com/MystenLabs/sui/tree/main/sui_programmability/examples/math): This introduces a RistrettoPoint struct, which represents an elliptic curve point on the Ristretto255 sub-group of curve25519. Currently, we support the addition and subtraction of elements in the Ristretto255 sub-group, and operations of scalars within the field of curve25519.
  * HKDF, AES, ECDH

## Sui CLI
* Ability to use Sui CLI on full nodes ([issue](https://github.com/MystenLabs/sui/pull/4404))
* Generate keypair now requires signature scheme flag
  * `sui keytool generate ed25519` or `sui keytool generate secp256k1`
  * `sui client new-address ed25519` or  `sui client new-address secp256k1`
* Key derivation for Ed25519 following SLIP-0010
