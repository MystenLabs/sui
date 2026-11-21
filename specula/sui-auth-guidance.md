# Sui Validator: Transaction Authorization, Voting, and Validity Invariants

## Required

### Goal

- Assess whether a transaction that a correct validator would refuse can still be executed
  and committed to a certified checkpoint. A correct validator refuses a transaction when
  its signatures do not authorize every required signer, when it fails a validity check,
  or when it breaks a claim the protocol relies on for safety.
- The run should support a decision about two questions. Can an attacker get a transaction
  finalized by consensus without a valid authorization from the owner of every input? Can
  any validity invariant that execution or checkpointing relies on be bypassed?

### Scope

- In scope:
  - Signature verification: `crates/sui-core/src/signature_verifier.rs`
    (`verify_tx_with_current_aliases`, `verify_tx_require_no_aliases`, `verify_tx`, the
    signed-data cache keyed by `full_message_digest_with_alias_versions`, the zkLogin
    inputs cache, JWK updates), and `crates/sui-types/src` (`signature_verification.rs`,
    `signature.rs`, `multisig.rs`, `zk_login_authenticator.rs`,
    `passkey_authenticator.rs`, `verify_sender_signed_data_message_signatures` in
    `transaction.rs`).
  - Address aliases: `crates/sui-types/src/address_alias.rs`, alias version claims in
    `PlainTransactionWithClaims`, the V1 and V2 claim formats, the
    `fix_checkpoint_signature_mapping` switch, alias handling in
    `authority_per_epoch_store.rs` (deferred transactions with aliases, checkpoint
    signature mapping near line 2900), and checkpoint contents with alias versions in
    `checkpoints/mod.rs`.
  - Transaction voting: `crates/sui-core/src/consensus_validator.rs` (`verify_batch`,
    `validate_transactions`, `vote_transactions`, `vote_transaction`,
    `verify_immutable_object_claims`, `check_allowed_proposer`) and the vote semantics in
    `consensus/core/src/transaction_vote_tracker.rs` and `commit_finalizer.rs` (direct and
    indirect finalization, rejection at quorum reject stake, rejection by depth, GC bound,
    recovered rejected transactions after restart).
  - Validity checks: `TransactionData::validity_check` and the `validity_check` methods on
    `CallArg`, `ProgrammableTransaction`, and commands in `crates/sui-types/src/transaction.rs`;
    `crates/sui-transaction-checks/`; `transaction_input_loader.rs`;
    `transaction_deny_config_manager.rs` and deny config updates applied at verification;
    `gasless_rate_limiter.rs`; timestamp expiration (`check_timestamp_expiration_for_vote`,
    `check_not_before_min_timestamp`); and the checks in `authority_server.rs` (signing and
    submission) and `authority.rs` (`handle_vote_transaction` and the checks near lines
    1329, 2027, and 2376).
  - Every path by which a transaction reaches execution: user transactions finalized by
    consensus, transactions from a certified checkpoint executed by the checkpoint
    executor or state sync, system transactions (consensus commit prologue, authenticator
    state update, randomness state update, settlement and barrier transactions, end of
    epoch transactions), and transactions that Sui rewrites
    (`accumulators/transaction_rewriting.rs`, coin reservations).
- Boundaries/exclusions:
  - Treat cryptographic primitives as ideal. A signature verifies if and only if the key
    holder signed the message. Do not model curve arithmetic, Groth16, or hash collisions.
    Do model which bytes are signed, which digest is cached, and which key or address a
    signature is checked against.
  - Treat Move execution as a black box, except for the checks the adapter performs on
    inputs and ownership before execution, and the alias state that Move code updates.
  - Exclude DAG construction and leader election. Model consensus as delivering blocks with
    per-transaction reject votes from each authority, and commits in the same order to
    every validator.
  - Exclude denial of service and resource exhaustion, unless a bound is part of a safety
    argument (for example, an input count limit that `transaction_input_loader.rs` relies
    on).

### Priority Questions

1. Can a transaction be finalized and executed when fewer than f+1 honest validators
   checked its signatures and validity against a state that authorizes it?
   Expected behavior: a transaction is finalized only with less than quorum reject stake,
   and acceptance stake from honest validators implies that at least one honest validator
   verified the signatures and ran `validity_check`. Rejection by depth or GC never turns
   into acceptance.
2. Can a change in alias state between voting and execution let a key that is no longer an
   authorized alias (or never was one) sign for an address? This includes alias removal
   while a transaction is in consensus or deferred, alias version claims that differ across
   validators, the V1 and V2 claim formats, epoch change, and a hit in the signed-data cache
   under a stale alias version.
   Expected behavior: the alias versions used to verify a transaction are the versions
   recorded with it, every honest validator that accepts the transaction verifies it
   against those versions, and execution and checkpoint signature mapping use the same
   versions.
3. Which validity invariants do execution, input loading, the checkpoint builder, or
   effects certification assume, and is each one enforced on every path a transaction can
   take to execution? Paths include voting, checkpoint execution, state sync replay,
   deferral across rounds and epochs, rewriting, and system transactions.
   Expected behavior: each invariant is checked on every path, or the path cannot carry
   a transaction that violates it. List each invariant, the code that relies on it, and
   the code that enforces it.
4. Can a check that depends on time-varying state (protocol config, epoch, reference gas
   price, deny config, JWK set, timestamp expiration, allowed proposers, overload state)
   pass at voting and stop holding at execution in a way that breaks safety, or differ
   across validators in a way that breaks determinism?
   Expected behavior: a check that execution relies on is either re-checked
   deterministically after consensus, or its inputs cannot change between voting and
   execution. Checks that only gate voting may differ across validators without breaking
   safety.
5. Can claims attached to a transaction (immutable object claims, alias version claims,
   allowed proposers) cause a correct validator to lock, skip, or treat objects in a way
   that the transaction's signers did not authorize?
   Expected behavior: a claim is either verified by the voting validator or has no effect on
   safety when it is false.

### Must-cover Interactions

- `vote_transaction` (`validity_check`, timestamp expiration, overload, alias
  verification, `handle_vote_transaction`, immutable object claims) ↔ reject votes in
  `transaction_vote_tracker.rs` ↔ finalization and rejection in `commit_finalizer.rs` ↔
  `consensus_handler.rs` processing of finalized and rejected transactions.
- `verify_tx_with_current_aliases` at voting ↔ alias object updates by executed
  transactions ↔ `WithAliases` claims carried into the consensus handler, deferred
  transaction storage, and checkpoint contents.
- The signed-data cache and the zkLogin inputs cache ↔ JWK set changes, epoch change, and
  `clear_signature_cache`.
- `validity_check` at signing in `authority_server.rs` ↔ `validity_check` at voting ↔
  absence of `validity_check` on the checkpoint execution path ↔ code in
  `transaction_input_loader.rs` and the adapter that relies on its bounds.
- System transactions and rewritten transactions ↔ checks that user transactions must
  pass. Confirm that a user cannot submit a transaction kind reserved for the system.
- Consider: a Byzantine minority of validators (at most f by stake) that vote to accept
  anything and propose arbitrary blocks; a validator restart that recovers rejected
  transactions; epoch change while a transaction is pending; protocol upgrade that enables
  a new signature scheme or check.

### Assumptions

- At most f of the 3f+1 stake is Byzantine. Byzantine validators can vote arbitrarily and
  include any bytes in their blocks.
- Honest validators may hold different local state at voting time (alias objects, deny
  config, clocks, overload), because voting runs before execution catches up.
- Consensus delivers the same commits in the same order to every honest validator.
- The checked-out implementation is ground truth. The comment in `vote_transaction` says
  `validity_check` and `verify_transaction` are not required to agree across validators;
  test the consequences of that statement.

## Optional

### Known Incidents and References

- #28226: added an input-count check to `ProgrammableTransaction::validity_check`.
- #28507: multisig member signatures stored in committee order in `MultiSig::combine`.
- #28270, #28413: ML-DSA-65 membership in multisig and standalone verification were not
  gated by protocol flags.
- #28652, #28460: secp256r1 encoding and `ecrecover` checks added behind protocol flags.
- #28542: timestamp-based transaction expiration.
- #27862, #28157: allowed proposers.
- `fix_checkpoint_signature_mapping`: the change from V1 to V2 alias claim formats.
- Use `git log` on `crates/sui-core/src/consensus_validator.rs`,
  `crates/sui-core/src/signature_verifier.rs`, `crates/sui-types/src/signature_verification.rs`,
  and `crates/sui-types/src/transaction.rs` for more history.

### Suggested Starting Points

- `crates/sui-core/src/consensus_validator.rs`: `vote_transaction`.
- `crates/sui-core/src/signature_verifier.rs`: `verify_tx_with_current_aliases`.
- `crates/sui-types/src/transaction.rs`: `TransactionData::validity_check`,
  `verify_sender_signed_data_message_signatures`.
- `consensus/core/src/commit_finalizer.rs` and `transaction_vote_tracker.rs`.
- `crates/sui-core/src/consensus_handler.rs`: handling of `UserTransactionV2` and alias
  claims near line 3274.
- `crates/sui-core/src/authority/authority_per_epoch_store.rs`:
  `verify_transaction_with_current_aliases` and the checkpoint signature mapping near
  line 2900.

### Harness Notes

- Build and test commands: `cargo check -p sui-core`, and
  `SUI_SKIP_SIMTESTS=1 cargo nextest run -p sui-core --lib <filter>`. The first build takes
  several minutes; use timeouts of at least 10 minutes.
- Existing tests: the test module in `consensus_validator.rs`, `signature_verifier.rs`
  tests, `crates/sui-types/src/unit_tests/`, and the `commit_finalizer.rs` tests in
  consensus.
- Simulation tests in `crates/sui-e2e-tests/tests/` must run with `cargo simtest`. The fail
  point `consensus-validator-always-report-aliases-changed` already exists.
- Each confirmation agent builds into its own cargo target directory of 50-90 GB. Check
  disk usage during phase 4.
