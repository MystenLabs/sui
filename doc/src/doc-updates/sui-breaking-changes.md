---
title: Sui Breaking Changes in Release .28
---

The next release of Sui, release 0.28, includes the breaking changes described in this topic. A breaking change is one that introduces new, or changed, Sui functionality that causes existing apps and implementations to stop functioning as expected.

To learn how to update your project to work with the changes introduced in release .28, see the [Sui Migration Guide](sui-migration-guide.md).

New entries added 03/20/23.

**[Major breaking change]** - Sui now calculates `SuiAddress` using the first 32 bytes of the Blake2b hash of `flag || pubkey` instead of the SHA3_256 hash. See [PR 9262](https://github.com/MystenLabs/sui/pull/9262) for more information.

---

**[Major breaking change]** - This release replaces the `sui_getValidators` and `sui_getSuiSystemState` functions with a new `sui_getLatestSuiSystemState` function. The new function returns a flattened type that contains all information from the latest `SuiSystemState` object on-chain with type `SuiSystemStateSummary`. It also contains a vector of type `SuiValidatorSummary` that summarizes information from each validator, including: metadata, staking pool, and other data. The release also adds a `p2p_address` to each validator’s metadata. The value for the field is the address the validator used for p2p activities, such as state sync.

---

**[Major breaking change]** - This release changes the serialization format of Sui object types. Sui now uses a more compact serialization format for common types such as `Coin<SUI>`, `Coin<T>`, and `StakedSui`, reducing object size by up to 40%. This lowers storage gas costs for objects of these types. This doesn’t effect clients using JSON-RPC API read functions, but clients that read raw Sui objects directly need to understand the new type encoding. Note that the encoding of Sui Move structs remains unchanged. See [PR 9055](https://github.com/MystenLabs/sui/pull/9055) for more details.

---

**[Major breaking change]** - The `sui_getObject` endpoint now takes an additional configuration parameter of type `SuiObjectDataOptions` to control which fields the endpoint retrieves. By default, the endpoint retrieves only object references unless the client request explicitly specifies other data, such as `type`, `owner`, or `bcs`. To learn more, see [PR 8817](https://github.com/MystenLabs/sui/pull/8817)

---

**[Major breaking change]** - The ID leak verifier that governs usage of `UID`s in Sui Move code has been rewritten and flipped. New objects must now get “fresh” `UID`s created in the function where the object is made, but when the object’s struct is destroyed, the UID can be stored as if the object was wrapped (but without it's contents). In contrast, the previous rules stated that the `UID` could come from anywhere, but must have been destroyed when the object was unpacked. Sui makes this change to make using dynamic fields a bit more ergonomic, so you do not always need a `Bag` or `Table` if you want to retain access to dynamic fields after unpacking an object into its constituent fields. See [PR 8026](https://github.com/MystenLabs/sui/pull/8026) for details and a migration example.

---

**[Major breaking change]** - The new Programmable Transactions feature introduces a new type of transaction that replaces both batch transactions and normal transactions (with the exception of special system transactions). These transactions allow for a series of Commands (mini transactions of sorts) to be executed, where the results of commands can be used in following commands. For more information, see the [Programmable Transactions RFC](https://forums.sui.io/t/rfc-planned-feature-programmable-transactions/3823).

---

**[Major breaking change]** - `SuiAddress` and `ObjectID` are now 32 bytes long instead of 20 bytes (in hex, the `len` increases from 40 to 64). If your software interacts with any `ObjectID` and `SuiAddress`, you must update it to use updated addresses and redeploy it. [PR 8542](https://github.com/MystenLabs/sui/pull/8542).

---

**[Breaking change]** - This release introduces several limits on transactions and transaction executions. Many of these limits are subject to change prior to Sui Mainnet. To view a list of limits in release .28, see the [source code](https://github.com/MystenLabs/sui/blob/main/crates/sui-protocol-config/src/lib.rs#L716).

---

**[Breaking change]** - Changes to Gas Budget to use SUI rather than gas units. This removes the concept of gas units from any user-related API operations. This does not change the format of `TransactionData` (u64). This is not a breaking change in the sense that the current format no longer works, but rather requires you to reconsider how you use gas budgets.

---

**[Breaking change]** - Prior to release .28, transactions required a single coin to pay for gas. This sometimes resulted in users needing to make separate transactions (such as `PaySui`) to merge coins before completing a transaction, which can also increase the cost of the transaction. This release changes the field value type in `TransactionData` from `gas_payment: ObjectRef` to `gas_payment: Vec<ObjectRef>`, where `Vec<ObjectRef>` is a non-empty vector of owned SUI objects. This combines all of the coins into a single coin, using the `ObjectID` of the first coin in the vector as the coin containing the merge.

---

**[Breaking change]** - `ecdsa_k1::ecrecover` and `ecdsa_k1::secp256k1_verify` now require you to input the raw message instead of a hashed message. You must also include the hash_function name represented by u8. See [PR 7773](https://github.com/MystenLabs/sui/pull/7773) for more details.

---

**[Breaking change]** The `ValidatorMetadata` function now includes a p2p_address field. The value for the field is the address the validator used for p2p activities, such as state sync. To learn more, see [PR 8636](https://github.com/MystenLabs/sui/pull/8636).

---

**[Transaction format breaking change]** - Adds a new expiration field to `TransactionData` to allow for users to specify a time that a transaction should expire, meaning it is no longer eligible to sign and execute by validators. In this release, the only supported value for the expiration field is epoch`. If not provided, no expiration is set for the associated transaction.

---

**[Minor breaking change]** - This release modifies the format for `ConsensusCommitPrologue` transactions. This is a system-generated transaction that updates timestamp on the `Clock` object, allowing Sui Move smart contracts to read up-to-date timestamps from the blockchain.

---

**[Minor breaking change]** - Removes `bulletproofs` and `elliptic_curve` modules from the Sui Framework. For more information, see [PR 8660](https://github.com/MystenLabs/sui/pull/8660).

---

**[Minor breaking change]** - Removes `Randomness` from the Sui Framework and the `sui_tblsSignRandomnessObject` JSON RPC. See [PR 8977](https://github.com/MystenLabs/sui/pull/8977) for more information.

---

**[Minor breaking change]** - This changes the genesis snapshot since the generation for a PoP changed. It also removes Sui Move APIs `bls12381::bls12381_min_sig_verify_with_domain`, and `validator::verify_proof_of_possession` because now all validate PoP is now done in `validator::validate_metadata`.

---

**[Major API breaking changes]** - `GetTransaction` API refactoring

- [RPC] `sui_getTransactionBlock` and `sui_multiGetTransaction` now take in an additional optional parameter called `options` that specifies which fields to retrieve (such as `transaction`, `effects`, `events`, etc). By default, these operations return only the transaction digest.
- [TS SDK] Renamed `provider.getTransactionWithEffects` to `provider.getTransactionResponse`. The new method takes in an additional parameter, `SuiTransactionBlockResponseOptions`, to configure which fields to retrieve (such as `transaction`, `effects`, `events`, etc). By default, this method returns only the transaction digest.

For more information, see [PR 8888](https://github.com/MystenLabs/sui/pull/8888).

---

**[Major API breaking changes] sui_executeTransactionBlock refactoring**

- Removed `sui_executeTransactionBlockSerializedSig` and `sui_submitTransaction` operations.
- The `sui_executeTransactionBlock` operation now takes a vector of signatures instead of a single signature to support Sponsored Transactions.

To learn more, see [PR 9068](https://github.com/MystenLabs/sui/pull/9068).

---

**[RPC API breaking change]** - Various changes in JSON-RPC governance API:

- updated `sui_getDelegatedStakes` to the new staking flow
- grouped all `StakedSui` by staking pool to reduce duplicate validator info in the response
- improve `ValidatorMetadata` JSON response to make it more human-readable, which affects `getSuiSystemState` as well.
- make `SuiSystemState` JSON response `camelCased`
- added `--epoch-duration-ms` option to Sui genesis for configuring localnet epoch duration

For more information, see [PR 8848](https://github.com/MystenLabs/sui/pull/8848).

---

Added 03/20/23

**[API breaking change]** - A valid signature must be committed to the Blake2b hash of the message before passing to any signing APIs. If a signature is created elsewhere, please ensure the transaction data is hashed first. For more information, see [PR 9561](https://github.com/MystenLabs/sui/pull/9561).

---

**[API breaking change]** - This release removes locked coin staking functionality, and changes the layout of the StakedSui object to remove the locking period field. For more information, see [PR 9046](https://github.com/MystenLabs/sui/pull/9046).

---

**[API breaking change]** - All functions that include _delegation_ in their name are renamed to use _stake_ instead. For example, `request_add_delegation` is now `request_add_stake`. See [PR 9059](https://github.com/MystenLabs/sui/pull/9059) for details.

---

**[API breaking change]** - This release replaces `SuiCertifiedTransaction` with `SuiTransactionBlock` in `SuiTransactionBlockResponse`. This is because validators can no longer guarantee to return a transaction certificate. This release also unifies `SuiTransactionBlockResponse` and `SuiExecuteTransactionResponse` to simplify the API. See [PR 8369](https://github.com/MystenLabs/sui/pull/8369) for more information.

---

**[API breaking change]** - Updates the structure for dynamic field names to make it easier to use in `sui_getDynamicFieldObject`. For more details, see [PR 7318](https://github.com/MystenLabs/sui/pull/7318)

---

**[API breaking change]** - This release removes the `request_switch_delegation` function from the Transaction Builder API. It also removes the `pending_delegation_switches` field from the validator set type in the Sui SDK. See [PR 8435](https://github.com/MystenLabs/sui/pull/8435) for more information.

---

**[API breaking change]** - To reduce the size of Sui Full node synchronization payloads, this release removes events from `TransactionEffect`. The events are still included in the `SuiTransactionBlockResponse` returned by `sui_getTransactionBlock` and `sui_submitTransaction` endpoints. For more information, see [PR 7822](https://github.com/MystenLabs/sui/pull/7822).

---

**[API breaking change]** - The `StakedSui` object now includes the ID of the staking pool, `pool_id`. For more information, see [PR 8371](https://github.com/MystenLabs/sui/pull/8371).
