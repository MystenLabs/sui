// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cell::RefCell,
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering},
};

use clap::*;
use move_vm_config::verifier::VerifierConfig;
use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use sui_protocol_config_macros::{
    ProtocolConfigAccessors, ProtocolConfigFeatureFlagsGetters, ProtocolConfigOverride,
};
use tracing::{info, warn};

/// The minimum and maximum protocol versions supported by this build.
const MIN_PROTOCOL_VERSION: u64 = 1;
const MAX_PROTOCOL_VERSION: u64 = 74;

// Record history of protocol version allocations here:
//
// Version 1: Original version.
// Version 2: Framework changes, including advancing epoch_start_time in safemode.
// Version 3: gas model v2, including all sui conservation fixes. Fix for loaded child object
//            changes, enable package upgrades, add limits on `max_size_written_objects`,
//            `max_size_written_objects_system_tx`
// Version 4: New reward slashing rate. Framework changes to skip stake susbidy when the epoch
//            length is short.
// Version 5: Package upgrade compatibility error fix. New gas cost table. New scoring decision
//            mechanism that includes up to f scoring authorities.
// Version 6: Change to how bytes are charged in the gas meter, increase buffer stake to 0.5f
// Version 7: Disallow adding new abilities to types during package upgrades,
//            disable_invariant_violation_check_in_swap_loc,
//            disable init functions becoming entry,
//            hash module bytes individually before computing package digest.
// Version 8: Disallow changing abilities and type constraints for type parameters in structs
//            during upgrades.
// Version 9: Limit the length of Move idenfitiers to 128.
//            Disallow extraneous module bytes,
//            advance_to_highest_supported_protocol_version,
// Version 10:increase bytecode verifier `max_verifier_meter_ticks_per_function` and
//            `max_meter_ticks_per_module` limits each from 6_000_000 to 16_000_000. sui-system
//            framework changes.
// Version 11: Introduce `std::type_name::get_with_original_ids` to the system frameworks. Bound max depth of values within the VM.
// Version 12: Changes to deepbook in framework to add API for querying marketplace.
//             Change NW Batch to use versioned metadata field.
//             Changes to sui-system package to add PTB-friendly unstake function, and minor cleanup.
// Version 13: System package change deprecating `0xdee9::clob` and `0xdee9::custodian`, replaced by
//             `0xdee9::clob_v2` and `0xdee9::custodian_v2`.
// Version 14: Introduce a config variable to allow charging of computation to be either
//             bucket base or rounding up. The presence of `gas_rounding_step` (or `None`)
//             decides whether rounding is applied or not.
// Version 15: Add reordering of user transactions by gas price after consensus.
//             Add `sui::table_vec::drop` to the framework via a system package upgrade.
// Version 16: Enabled simplified_unwrap_then_delete feature flag, which allows the execution engine
//             to no longer consult the object store when generating unwrapped_then_deleted in the
//             effects; this also allows us to stop including wrapped tombstones in accumulator.
//             Add self-matching prevention for deepbook.
// Version 17: Enable upgraded multisig support.
// Version 18: Introduce execution layer versioning, preserve all existing behaviour in v0.
//             Gas minimum charges moved to be a multiplier over the reference gas price. In this
//             protocol version the multiplier is the same as the lowest bucket of computation
//             such that the minimum transaction cost is the same as the minimum computation
//             bucket.
//             Add a feature flag to indicate the changes semantics of `base_tx_cost_fixed`.
// Version 19: Changes to sui-system package to enable liquid staking.
//             Add limit for total size of events.
//             Increase limit for number of events emitted to 1024.
// Version 20: Enables the flag `narwhal_new_leader_election_schedule` for the new narwhal leader
//             schedule algorithm for enhanced fault tolerance and sets the bad node stake threshold
//             value. Both values are set for all the environments except mainnet.
// Version 21: ZKLogin known providers.
// Version 22: Child object format change.
// Version 23: Enabling the flag `narwhal_new_leader_election_schedule` for the new narwhal leader
//             schedule algorithm for enhanced fault tolerance and sets the bad node stake threshold
//             value for mainnet.
// Version 24: Re-enable simple gas conservation checks.
//             Package publish/upgrade number in a single transaction limited.
//             JWK / authenticator state flags.
// Version 25: Add sui::table_vec::swap and sui::table_vec::swap_remove to system packages.
// Version 26: New gas model version.
//             Add support for receiving objects off of other objects in devnet only.
// Version 28: Add sui::zklogin::verify_zklogin_id and related functions to sui framework.
//             Enable transaction effects v2 in devnet.
// Version 29: Add verify_legacy_zklogin_address flag to sui framework, this add ability to verify
//             transactions from a legacy zklogin address.
// Version 30: Enable Narwhal CertificateV2
//             Add support for random beacon.
//             Enable transaction effects v2 in testnet.
//             Deprecate supported oauth providers from protocol config and rely on node config
//             instead.
//             In execution, has_public_transfer is recomputed when loading the object.
//             Add support for shared obj deletion and receiving objects off of other objects in devnet only.
// Version 31: Add support for shared object deletion in devnet only.
//             Add support for getting object ID referenced by receiving object in sui framework.
//             Create new execution layer version, and preserve previous behavior in v1.
//             Update semantics of `sui::transfer::receive` and add `sui::transfer::public_receive`.
// Version 32: Add delete functions for VerifiedID and VerifiedIssuer.
//             Add sui::token module to sui framework.
//             Enable transfer to object in testnet.
//             Enable Narwhal CertificateV2 on mainnet
//             Make critbit tree and order getters public in deepbook.
// Version 33: Add support for `receiving_object_id` function in framework
//             Hardened OTW check.
//             Enable transfer-to-object in mainnet.
//             Enable shared object deletion in testnet.
//             Enable effects v2 in mainnet.
// Version 34: Framework changes for random beacon.
// Version 35: Add poseidon hash function.
//             Enable coin deny list.
// Version 36: Enable group operations native functions in devnet.
//             Enable shared object deletion in mainnet.
//             Set the consensus accepted transaction size and the included transactions size in the proposed block.
// Version 37: Reject entry functions with mutable Random.
// Version 38: Introduce limits for binary tables size.
// Version 39: Allow skipped epochs for randomness updates.
//             Extra version to fix `test_upgrade_compatibility` simtest.
// Version 40:
// Version 41: Enable group operations native functions in testnet and mainnet (without msm).
// Version 42: Migrate sui framework and related code to Move 2024
// Version 43: Introduce the upper bound delta config for a zklogin signature's max epoch.
//             Introduce an explicit parameter for the tick limit per package (previously this was
//             represented by the parameter for the tick limit per module).
// Version 44: Enable consensus fork detection on mainnet.
//             Switch between Narwhal and Mysticeti consensus in tests, devnet and testnet.
// Version 45: Use tonic networking for Mysticeti consensus.
//             Set min Move binary format version to 6.
//             Enable transactions to be signed with zkLogin inside multisig signature.
//             Add native bridge.
//             Enable native bridge in devnet
//             Enable Leader Scoring & Schedule Change for Mysticeti consensus on testnet.
// Version 46: Enable native bridge in testnet
//             Enable resharing at the same initial shared version.
// Version 47: Deepbook changes (framework update)
// Version 48: Use tonic networking for Mysticeti.
//             Resolve Move abort locations to the package id instead of the runtime module ID.
//             Enable random beacon in testnet.
//             Use new VM when verifying framework packages.
// Version 49: Enable Move enums on devnet.
//             Enable VDF in devnet
//             Enable consensus commit prologue V3 in devnet.
//             Run Mysticeti consensus by default.
// Version 50: Add update_node_url to native bridge,
//             New Move stdlib integer modules
//             Enable checkpoint batching in testnet.
//             Prepose consensus commit prologue in checkpoints.
//             Set number of leaders per round for Mysticeti commits.
// Version 51: Switch to DKG V1.
//             Enable deny list v2 on devnet.
// Version 52: Emit `CommitteeMemberUrlUpdateEvent` when updating bridge node url.
//             std::config native functions.
//             Modified sui-system package to enable withdrawal of stake before it becomes active.
//             Enable soft bundle in devnet and testnet.
//             Core macro visibility in sui core framework.
//             Enable checkpoint batching in mainnet.
//             Enable Mysticeti on mainnet.
//             Enable Leader Scoring & Schedule Change for Mysticeti consensus on mainnet.
//             Turn on count based shared object congestion control in devnet.
//             Enable consensus commit prologue V3 in testnet.
//             Enable enums on testnet.
//             Add support for passkey in devnet.
//             Enable deny list v2 on testnet and mainnet.
// Version 53: Add feature flag to decide whether to attempt to finalize bridge committee
//             Enable consensus commit prologue V3 on testnet.
//             Turn on shared object congestion control in testnet.
//             Update stdlib natives costs
// Version 54: Enable random beacon on mainnet.
//             Enable soft bundle on mainnet.
// Version 55: Enable enums on mainnet.
//             Rethrow serialization type layout errors instead of converting them.
// Version 56: Enable bridge on mainnet.
//             Note: do not use version 56 for any new features.
// Version 57: Reduce minimum number of random beacon shares.
// Version 58: Optimize boolean binops
//             Finalize bridge committee on mainnet.
//             Switch to distributed vote scoring in consensus in devnet
// Version 59: Enable round prober in consensus.
// Version 60: Validation of public inputs for Groth16 verification.
//             Enable configuration of maximum number of type nodes in a type layout.
// Version 61: Switch to distributed vote scoring in consensus in testnet
//             Further reduce minimum number of random beacon shares.
//             Add feature flag for Mysticeti fastpath.
// Version 62: Makes the event's sending module package upgrade-aware.
// Version 63: Enable gas based congestion control in consensus commit.
// Version 64: Revert congestion control change.
// Version 65: Enable distributed vote scoring in mainnet.
// Version 66: Revert distributed vote scoring in mainnet.
//             Framework fix for fungible staking book-keeping.
// Version 67: Re-enable distributed vote scoring in mainnet.
// Version 68: Add G1Uncompressed group to group ops.
//             Update to Move stdlib.
//             Enable gas based congestion control with overage.
//             Further reduce minimum number of random beacon shares.
//             Disallow adding new modules in `deps-only` packages.
// Version 69: Sets number of rounds allowed for fastpath voting in consensus.
//             Enable smart ancestor selection in devnet.
//             Enable G1Uncompressed group in testnet.
// Version 70: Enable smart ancestor selection in testnet.
//             Enable probing for accepted rounds in round prober in testnet
//             Add new gas model version to update charging of native functions.
//             Add std::uq64_64 module to Move stdlib.
//             Improve gas/wall time efficiency of some Move stdlib vector functions
// Version 71: [SIP-45] Enable consensus amplification.
// Version 72: Fix issue where `convert_type_argument_error` wasn't being used in all cases.
//             Max gas budget moved to 50_000 SUI
//             Max gas price moved to 50 SUI
//             Variants as type nodes.
// Version 73: Enable new marker table version.
//             Enable consensus garbage collection and new commit rule for devnet.
//             Enable zstd compression for consensus tonic network in testnet.
//             Enable smart ancestor selection in mainnet.
//             Enable probing for accepted rounds in round prober in mainnet
// Version 74: Enable load_nitro_attestation move function in sui framework in devnet.
//             Enable all gas costs for load_nitro_attestation.
//             Enable zstd compression for consensus tonic network in mainnet.
//             Enable the new commit rule for devnet.

#[derive(Copy, Clone, Debug, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProtocolVersion(u64);

impl ProtocolVersion {
    // The minimum and maximum protocol version supported by this binary. Counterintuitively, this constant may
    // change over time as support for old protocol versions is removed from the source. This
    // ensures that when a new network (such as a testnet) is created, its genesis committee will
    // use a protocol version that is actually supported by the binary.
    pub const MIN: Self = Self(MIN_PROTOCOL_VERSION);

    pub const MAX: Self = Self(MAX_PROTOCOL_VERSION);

    #[cfg(not(msim))]
    const MAX_ALLOWED: Self = Self::MAX;

    // We create one additional "fake" version in simulator builds so that we can test upgrades.
    #[cfg(msim)]
    pub const MAX_ALLOWED: Self = Self(MAX_PROTOCOL_VERSION + 1);

    pub fn new(v: u64) -> Self {
        Self(v)
    }

    pub const fn as_u64(&self) -> u64 {
        self.0
    }

    // For serde deserialization - we don't define a Default impl because there isn't a single
    // universally appropriate default value.
    pub fn max() -> Self {
        Self::MAX
    }
}

impl From<u64> for ProtocolVersion {
    fn from(v: u64) -> Self {
        Self::new(v)
    }
}

impl std::ops::Sub<u64> for ProtocolVersion {
    type Output = Self;
    fn sub(self, rhs: u64) -> Self::Output {
        Self::new(self.0 - rhs)
    }
}

impl std::ops::Add<u64> for ProtocolVersion {
    type Output = Self;
    fn add(self, rhs: u64) -> Self::Output {
        Self::new(self.0 + rhs)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Copy, PartialOrd, Ord, Eq, ValueEnum)]
pub enum Chain {
    Mainnet,
    Testnet,
    Unknown,
}

impl Default for Chain {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Chain {
    pub fn as_str(self) -> &'static str {
        match self {
            Chain::Mainnet => "mainnet",
            Chain::Testnet => "testnet",
            Chain::Unknown => "unknown",
        }
    }
}

pub struct Error(pub String);

// TODO: There are quite a few non boolean values in the feature flags. We should move them out.
/// Records on/off feature flags that may vary at each protocol version.
#[derive(Default, Clone, Serialize, Deserialize, Debug, ProtocolConfigFeatureFlagsGetters)]
struct FeatureFlags {
    // Add feature flags here, e.g.:
    // new_protocol_feature: bool,
    #[serde(skip_serializing_if = "is_false")]
    package_upgrades: bool,
    // If true, validators will commit to the root state digest
    // in end of epoch checkpoint proposals
    #[serde(skip_serializing_if = "is_false")]
    commit_root_state_digest: bool,
    // Pass epoch start time to advance_epoch safe mode function.
    #[serde(skip_serializing_if = "is_false")]
    advance_epoch_start_time_in_safe_mode: bool,
    // If true, apply the fix to correctly capturing loaded child object versions in execution's
    // object runtime.
    #[serde(skip_serializing_if = "is_false")]
    loaded_child_objects_fixed: bool,
    // If true, treat missing types in the upgraded modules when creating an upgraded package as a
    // compatibility error.
    #[serde(skip_serializing_if = "is_false")]
    missing_type_is_compatibility_error: bool,
    // If true, then the scoring decision mechanism will not get disabled when we do have more than
    // f low scoring authorities, but it will simply flag as low scoring only up to f authorities.
    #[serde(skip_serializing_if = "is_false")]
    scoring_decision_with_validity_cutoff: bool,

    // DEPRECATED: this was an ephemeral feature flag only used by consensus handler, which has now
    // been deployed everywhere.
    #[serde(skip_serializing_if = "is_false")]
    consensus_order_end_of_epoch_last: bool,

    // Disallow adding abilities to types during package upgrades.
    #[serde(skip_serializing_if = "is_false")]
    disallow_adding_abilities_on_upgrade: bool,
    // Disables unnecessary invariant check in the Move VM when swapping the value out of a local
    #[serde(skip_serializing_if = "is_false")]
    disable_invariant_violation_check_in_swap_loc: bool,
    // advance to highest supported protocol version at epoch change, instead of the next consecutive
    // protocol version.
    #[serde(skip_serializing_if = "is_false")]
    advance_to_highest_supported_protocol_version: bool,
    // If true, disallow entry modifiers on entry functions
    #[serde(skip_serializing_if = "is_false")]
    ban_entry_init: bool,
    // If true, hash module bytes individually when calculating package digests for upgrades
    #[serde(skip_serializing_if = "is_false")]
    package_digest_hash_module: bool,
    // If true, disallow changing struct type parameters during package upgrades
    #[serde(skip_serializing_if = "is_false")]
    disallow_change_struct_type_params_on_upgrade: bool,
    // If true, checks no extra bytes in a compiled module
    #[serde(skip_serializing_if = "is_false")]
    no_extraneous_module_bytes: bool,
    // If true, then use the versioned metadata format in narwhal entities.
    #[serde(skip_serializing_if = "is_false")]
    narwhal_versioned_metadata: bool,

    // Enable zklogin auth
    #[serde(skip_serializing_if = "is_false")]
    zklogin_auth: bool,
    // How we order transactions coming out of consensus before sending to execution.
    #[serde(skip_serializing_if = "ConsensusTransactionOrdering::is_none")]
    consensus_transaction_ordering: ConsensusTransactionOrdering,

    // Previously, the unwrapped_then_deleted field in TransactionEffects makes a distinction between
    // whether an object has existed in the store previously (i.e. whether there is a tombstone).
    // Such dependency makes effects generation inefficient, and requires us to include wrapped
    // tombstone in state root hash.
    // To prepare for effects V2, with this flag set to true, we simplify the definition of
    // unwrapped_then_deleted to always include unwrapped then deleted objects,
    // regardless of their previous state in the store.
    #[serde(skip_serializing_if = "is_false")]
    simplified_unwrap_then_delete: bool,
    // Enable upgraded multisig support
    #[serde(skip_serializing_if = "is_false")]
    upgraded_multisig_supported: bool,
    // If true minimum txn charge is a multiplier of the gas price
    #[serde(skip_serializing_if = "is_false")]
    txn_base_cost_as_multiplier: bool,

    // If true, the ability to delete shared objects is in effect
    #[serde(skip_serializing_if = "is_false")]
    shared_object_deletion: bool,

    // If true, then the new algorithm for the leader election schedule will be used
    #[serde(skip_serializing_if = "is_false")]
    narwhal_new_leader_election_schedule: bool,

    // A list of supported OIDC providers that can be used for zklogin.
    #[serde(skip_serializing_if = "is_empty")]
    zklogin_supported_providers: BTreeSet<String>,

    // If true, use the new child object format
    #[serde(skip_serializing_if = "is_false")]
    loaded_child_object_format: bool,

    #[serde(skip_serializing_if = "is_false")]
    enable_jwk_consensus_updates: bool,

    #[serde(skip_serializing_if = "is_false")]
    end_of_epoch_transaction_supported: bool,

    // Perform simple conservation checks keeping into account out of gas scenarios
    // while charging for storage.
    #[serde(skip_serializing_if = "is_false")]
    simple_conservation_checks: bool,

    // If true, use the new child object format type logging
    #[serde(skip_serializing_if = "is_false")]
    loaded_child_object_format_type: bool,

    // Enable receiving sent objects
    #[serde(skip_serializing_if = "is_false")]
    receive_objects: bool,

    // Enable random beacon protocol
    #[serde(skip_serializing_if = "is_false")]
    random_beacon: bool,

    // Enable bridge protocol
    #[serde(skip_serializing_if = "is_false")]
    bridge: bool,

    #[serde(skip_serializing_if = "is_false")]
    enable_effects_v2: bool,

    // If true, then use CertificateV2 in narwhal.
    #[serde(skip_serializing_if = "is_false")]
    narwhal_certificate_v2: bool,

    // If true, allow verify with legacy zklogin address
    #[serde(skip_serializing_if = "is_false")]
    verify_legacy_zklogin_address: bool,

    // Enable throughput aware consensus submission
    #[serde(skip_serializing_if = "is_false")]
    throughput_aware_consensus_submission: bool,

    // If true, recompute has_public_transfer from the type instead of what is stored in the object
    #[serde(skip_serializing_if = "is_false")]
    recompute_has_public_transfer_in_execution: bool,

    // If true, multisig containing zkLogin sig is accepted.
    #[serde(skip_serializing_if = "is_false")]
    accept_zklogin_in_multisig: bool,

    // If true, consensus prologue transaction also includes the consensus output digest.
    // It can be used to detect consensus output folk.
    #[serde(skip_serializing_if = "is_false")]
    include_consensus_digest_in_prologue: bool,

    // If true, use the hardened OTW check
    #[serde(skip_serializing_if = "is_false")]
    hardened_otw_check: bool,

    // If true allow calling receiving_object_id function
    #[serde(skip_serializing_if = "is_false")]
    allow_receiving_object_id: bool,

    // Enable the poseidon hash function
    #[serde(skip_serializing_if = "is_false")]
    enable_poseidon: bool,

    // If true, enable the coin deny list.
    #[serde(skip_serializing_if = "is_false")]
    enable_coin_deny_list: bool,

    // Enable native functions for group operations.
    #[serde(skip_serializing_if = "is_false")]
    enable_group_ops_native_functions: bool,

    // Enable native function for msm.
    #[serde(skip_serializing_if = "is_false")]
    enable_group_ops_native_function_msm: bool,

    // Enable nitro attestation.
    #[serde(skip_serializing_if = "is_false")]
    enable_nitro_attestation: bool,

    // Reject functions with mutable Random.
    #[serde(skip_serializing_if = "is_false")]
    reject_mutable_random_on_entry_functions: bool,

    // Controls the behavior of per object congestion control in consensus handler.
    #[serde(skip_serializing_if = "PerObjectCongestionControlMode::is_none")]
    per_object_congestion_control_mode: PerObjectCongestionControlMode,

    // The consensus protocol to be used for the epoch.
    #[serde(skip_serializing_if = "ConsensusChoice::is_narwhal")]
    consensus_choice: ConsensusChoice,

    // Consensus network to use.
    #[serde(skip_serializing_if = "ConsensusNetwork::is_anemo")]
    consensus_network: ConsensusNetwork,

    // Set the upper bound allowed for max_epoch in zklogin signature.
    #[serde(skip_serializing_if = "Option::is_none")]
    zklogin_max_epoch_upper_bound_delta: Option<u64>,

    // Controls leader scoring & schedule change in Mysticeti consensus.
    #[serde(skip_serializing_if = "is_false")]
    mysticeti_leader_scoring_and_schedule: bool,

    // Enable resharing of shared objects using the same initial shared version
    #[serde(skip_serializing_if = "is_false")]
    reshare_at_same_initial_version: bool,

    // Resolve Move abort locations to the package id instead of the runtime module ID.
    #[serde(skip_serializing_if = "is_false")]
    resolve_abort_locations_to_package_id: bool,

    // Enables the use of the Mysticeti committed sub dag digest to the `ConsensusCommitInfo` in checkpoints.
    // When disabled the default digest is used instead. It's important to have this guarded behind
    // a flag as it will lead to checkpoint forks.
    #[serde(skip_serializing_if = "is_false")]
    mysticeti_use_committed_subdag_digest: bool,

    // Enable VDF
    #[serde(skip_serializing_if = "is_false")]
    enable_vdf: bool,

    // Controls whether consensus handler should record consensus determined shared object version
    // assignments in consensus commit prologue transaction.
    // The purpose of doing this is to enable replaying transaction without transaction effects.
    // V2 also records initial shared versions for consensus objects.
    #[serde(skip_serializing_if = "is_false")]
    record_consensus_determined_version_assignments_in_prologue: bool,
    #[serde(skip_serializing_if = "is_false")]
    record_consensus_determined_version_assignments_in_prologue_v2: bool,

    // Run verification of framework upgrades using a new/fresh VM.
    #[serde(skip_serializing_if = "is_false")]
    fresh_vm_on_framework_upgrade: bool,

    // When set to true, the consensus commit prologue transaction will be placed first
    // in a consensus commit in checkpoints.
    // If a checkpoint contains multiple consensus commit, say [cm1][cm2]. The each commit's
    // consensus commit prologue will be the first transaction in each segment:
    //     [ccp1, rest cm1][ccp2, rest cm2]
    // The reason to prepose the prologue transaction is to provide information for transaction
    // cancellation.
    #[serde(skip_serializing_if = "is_false")]
    prepend_prologue_tx_in_consensus_commit_in_checkpoints: bool,

    // Set number of leaders per round for Mysticeti commits.
    #[serde(skip_serializing_if = "Option::is_none")]
    mysticeti_num_leaders_per_round: Option<usize>,

    // Enable Soft Bundle (SIP-19).
    #[serde(skip_serializing_if = "is_false")]
    soft_bundle: bool,

    // If true, enable the coin deny list V2.
    #[serde(skip_serializing_if = "is_false")]
    enable_coin_deny_list_v2: bool,

    // Enable passkey auth (SIP-9)
    #[serde(skip_serializing_if = "is_false")]
    passkey_auth: bool,

    // Use AuthorityCapabilitiesV2
    #[serde(skip_serializing_if = "is_false")]
    authority_capabilities_v2: bool,

    // Rethrow type layout errors during serialization instead of trying to convert them.
    #[serde(skip_serializing_if = "is_false")]
    rethrow_serialization_type_layout_errors: bool,

    // Use distributed vote leader scoring strategy in consensus.
    #[serde(skip_serializing_if = "is_false")]
    consensus_distributed_vote_scoring_strategy: bool,

    // Probe rounds received by peers from every authority.
    #[serde(skip_serializing_if = "is_false")]
    consensus_round_prober: bool,

    // Validate identifier inputs separately
    #[serde(skip_serializing_if = "is_false")]
    validate_identifier_inputs: bool,

    // Enables Mysticeti fastpath.
    #[serde(skip_serializing_if = "is_false")]
    mysticeti_fastpath: bool,

    // Makes the event's sending module version-aware.
    #[serde(skip_serializing_if = "is_false")]
    relocate_event_module: bool,

    // Enable uncompressed group elements in BLS123-81 G1
    #[serde(skip_serializing_if = "is_false")]
    uncompressed_g1_group_elements: bool,

    #[serde(skip_serializing_if = "is_false")]
    disallow_new_modules_in_deps_only_packages: bool,

    // Use smart ancestor selection in consensus.
    #[serde(skip_serializing_if = "is_false")]
    consensus_smart_ancestor_selection: bool,

    // Probe accepted rounds in round prober.
    #[serde(skip_serializing_if = "is_false")]
    consensus_round_prober_probe_accepted_rounds: bool,

    // Enable v2 native charging for natives.
    #[serde(skip_serializing_if = "is_false")]
    native_charging_v2: bool,

    // Enables the new logic for collecting the subdag in the consensus linearizer. The new logic does not stop the recursion at the highest
    // committed round for each authority, but allows to commit uncommitted blocks up to gc round (excluded) for that authority.
    #[serde(skip_serializing_if = "is_false")]
    consensus_linearize_subdag_v2: bool,

    // Properly convert certain type argument errors in the execution layer.
    #[serde(skip_serializing_if = "is_false")]
    convert_type_argument_error: bool,

    // Variants count as nodes
    #[serde(skip_serializing_if = "is_false")]
    variant_nodes: bool,

    // If true, enable zstd compression for consensus tonic network.
    #[serde(skip_serializing_if = "is_false")]
    consensus_zstd_compression: bool,
}

fn is_false(b: &bool) -> bool {
    !b
}

fn is_empty(b: &BTreeSet<String>) -> bool {
    b.is_empty()
}

/// Ordering mechanism for transactions in one Narwhal consensus output.
#[derive(Default, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum ConsensusTransactionOrdering {
    /// No ordering. Transactions are processed in the order they appear in the consensus output.
    #[default]
    None,
    /// Order transactions by gas price, highest first.
    ByGasPrice,
}

impl ConsensusTransactionOrdering {
    pub fn is_none(&self) -> bool {
        matches!(self, ConsensusTransactionOrdering::None)
    }
}

// The config for per object congestion control in consensus handler.
#[derive(Default, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum PerObjectCongestionControlMode {
    #[default]
    None, // No congestion control.
    TotalGasBudget,        // Use txn gas budget as execution cost.
    TotalTxCount,          // Use total txn count as execution cost.
    TotalGasBudgetWithCap, // Use txn gas budget as execution cost with a cap.
    ExecutionTimeEstimate, // Use execution time estimate as execution cost.
}

impl PerObjectCongestionControlMode {
    pub fn is_none(&self) -> bool {
        matches!(self, PerObjectCongestionControlMode::None)
    }
}

// Configuration options for consensus algorithm.
#[derive(Default, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum ConsensusChoice {
    #[default]
    Narwhal,
    SwapEachEpoch,
    Mysticeti,
}

impl ConsensusChoice {
    pub fn is_narwhal(&self) -> bool {
        matches!(self, ConsensusChoice::Narwhal)
    }
}

// Configuration options for consensus network.
#[derive(Default, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum ConsensusNetwork {
    #[default]
    Anemo,
    Tonic,
}

impl ConsensusNetwork {
    pub fn is_anemo(&self) -> bool {
        matches!(self, ConsensusNetwork::Anemo)
    }
}

/// Constants that change the behavior of the protocol.
///
/// The value of each constant here must be fixed for a given protocol version. To change the value
/// of a constant, advance the protocol version, and add support for it in `get_for_version` under
/// the new version number.
/// (below).
///
/// To add a new field to this struct, use the following procedure:
/// - Advance the protocol version.
/// - Add the field as a private `Option<T>` to the struct.
/// - Initialize the field to `None` in prior protocol versions.
/// - Initialize the field to `Some(val)` for your new protocol version.
/// - Add a public getter that simply unwraps the field.
/// - Two public getters of the form `field(&self) -> field_type`
///     and `field_as_option(&self) -> Option<field_type>` will be automatically generated for you.
/// Example for a field: `new_constant: Option<u64>`
/// ```rust,ignore
///      pub fn new_constant(&self) -> u64 {
///         self.new_constant.expect(Self::CONSTANT_ERR_MSG)
///     }
///      pub fn new_constant_as_option(&self) -> Option<u64> {
///         self.new_constant.expect(Self::CONSTANT_ERR_MSG)
///     }
/// ```
/// With `pub fn new_constant(&self) -> u64`, if the constant is accessed in a protocol version
/// in which it is not defined, the validator will crash. (Crashing is necessary because
/// this type of error would almost always result in forking if not prevented here).
/// If you don't want the validator to crash, you can use the
/// `pub fn new_constant_as_option(&self) -> Option<u64>` getter, which will
/// return `None` if the field is not defined at that version.
/// - If you want a customized getter, you can add a method in the impl.
#[skip_serializing_none]
#[derive(Clone, Serialize, Debug, ProtocolConfigAccessors, ProtocolConfigOverride)]
pub struct ProtocolConfig {
    pub version: ProtocolVersion,

    feature_flags: FeatureFlags,

    // ==== Transaction input limits ====
    /// Maximum serialized size of a transaction (in bytes).
    max_tx_size_bytes: Option<u64>,

    /// Maximum number of input objects to a transaction. Enforced by the transaction input checker
    max_input_objects: Option<u64>,

    /// Max size of objects a transaction can write to disk after completion. Enforce by the Sui adapter.
    /// This is the sum of the serialized size of all objects written to disk.
    /// The max size of individual objects on the other hand is `max_move_object_size`.
    max_size_written_objects: Option<u64>,
    /// Max size of objects a system transaction can write to disk after completion. Enforce by the Sui adapter.
    /// Similar to `max_size_written_objects` but for system transactions.
    max_size_written_objects_system_tx: Option<u64>,

    /// Maximum size of serialized transaction effects.
    max_serialized_tx_effects_size_bytes: Option<u64>,

    /// Maximum size of serialized transaction effects for system transactions.
    max_serialized_tx_effects_size_bytes_system_tx: Option<u64>,

    /// Maximum number of gas payment objects for a transaction.
    max_gas_payment_objects: Option<u32>,

    /// Maximum number of modules in a Publish transaction.
    max_modules_in_publish: Option<u32>,

    /// Maximum number of transitive dependencies in a package when publishing.
    max_package_dependencies: Option<u32>,

    /// Maximum number of arguments in a move call or a ProgrammableTransaction's
    /// TransferObjects command.
    max_arguments: Option<u32>,

    /// Maximum number of total type arguments, computed recursively.
    max_type_arguments: Option<u32>,

    /// Maximum depth of an individual type argument.
    max_type_argument_depth: Option<u32>,

    /// Maximum size of a Pure CallArg.
    max_pure_argument_size: Option<u32>,

    /// Maximum number of Commands in a ProgrammableTransaction.
    max_programmable_tx_commands: Option<u32>,

    // ==== Move VM, Move bytecode verifier, and execution limits ===
    /// Maximum Move bytecode version the VM understands. All older versions are accepted.
    move_binary_format_version: Option<u32>,
    min_move_binary_format_version: Option<u32>,

    /// Configuration controlling binary tables size.
    binary_module_handles: Option<u16>,
    binary_struct_handles: Option<u16>,
    binary_function_handles: Option<u16>,
    binary_function_instantiations: Option<u16>,
    binary_signatures: Option<u16>,
    binary_constant_pool: Option<u16>,
    binary_identifiers: Option<u16>,
    binary_address_identifiers: Option<u16>,
    binary_struct_defs: Option<u16>,
    binary_struct_def_instantiations: Option<u16>,
    binary_function_defs: Option<u16>,
    binary_field_handles: Option<u16>,
    binary_field_instantiations: Option<u16>,
    binary_friend_decls: Option<u16>,
    binary_enum_defs: Option<u16>,
    binary_enum_def_instantiations: Option<u16>,
    binary_variant_handles: Option<u16>,
    binary_variant_instantiation_handles: Option<u16>,

    /// Maximum size of the `contents` part of an object, in bytes. Enforced by the Sui adapter when effects are produced.
    max_move_object_size: Option<u64>,

    // TODO: Option<increase to 500 KB. currently, publishing a package > 500 KB exceeds the max computation gas cost
    /// Maximum size of a Move package object, in bytes. Enforced by the Sui adapter at the end of a publish transaction.
    max_move_package_size: Option<u64>,

    /// Max number of publish or upgrade commands allowed in a programmable transaction block.
    max_publish_or_upgrade_per_ptb: Option<u64>,

    /// Maximum gas budget in MIST that a transaction can use.
    max_tx_gas: Option<u64>,

    /// Maximum amount of the proposed gas price in MIST (defined in the transaction).
    max_gas_price: Option<u64>,

    /// The max computation bucket for gas. This is the max that can be charged for computation.
    max_gas_computation_bucket: Option<u64>,

    // Define the value used to round up computation gas charges
    gas_rounding_step: Option<u64>,

    /// Maximum number of nested loops. Enforced by the Move bytecode verifier.
    max_loop_depth: Option<u64>,

    /// Maximum number of type arguments that can be bound to generic type parameters. Enforced by the Move bytecode verifier.
    max_generic_instantiation_length: Option<u64>,

    /// Maximum number of parameters that a Move function can have. Enforced by the Move bytecode verifier.
    max_function_parameters: Option<u64>,

    /// Maximum number of basic blocks that a Move function can have. Enforced by the Move bytecode verifier.
    max_basic_blocks: Option<u64>,

    /// Maximum stack size value. Enforced by the Move bytecode verifier.
    max_value_stack_size: Option<u64>,

    /// Maximum number of "type nodes", a metric for how big a SignatureToken will be when expanded into a fully qualified type. Enforced by the Move bytecode verifier.
    max_type_nodes: Option<u64>,

    /// Maximum number of push instructions in one function. Enforced by the Move bytecode verifier.
    max_push_size: Option<u64>,

    /// Maximum number of struct definitions in a module. Enforced by the Move bytecode verifier.
    max_struct_definitions: Option<u64>,

    /// Maximum number of function definitions in a module. Enforced by the Move bytecode verifier.
    max_function_definitions: Option<u64>,

    /// Maximum number of fields allowed in a struct definition. Enforced by the Move bytecode verifier.
    max_fields_in_struct: Option<u64>,

    /// Maximum dependency depth. Enforced by the Move linker when loading dependent modules.
    max_dependency_depth: Option<u64>,

    /// Maximum number of Move events that a single transaction can emit. Enforced by the VM during execution.
    max_num_event_emit: Option<u64>,

    /// Maximum number of new IDs that a single transaction can create. Enforced by the VM during execution.
    max_num_new_move_object_ids: Option<u64>,

    /// Maximum number of new IDs that a single system transaction can create. Enforced by the VM during execution.
    max_num_new_move_object_ids_system_tx: Option<u64>,

    /// Maximum number of IDs that a single transaction can delete. Enforced by the VM during execution.
    max_num_deleted_move_object_ids: Option<u64>,

    /// Maximum number of IDs that a single system transaction can delete. Enforced by the VM during execution.
    max_num_deleted_move_object_ids_system_tx: Option<u64>,

    /// Maximum number of IDs that a single transaction can transfer. Enforced by the VM during execution.
    max_num_transferred_move_object_ids: Option<u64>,

    /// Maximum number of IDs that a single system transaction can transfer. Enforced by the VM during execution.
    max_num_transferred_move_object_ids_system_tx: Option<u64>,

    /// Maximum size of a Move user event. Enforced by the VM during execution.
    max_event_emit_size: Option<u64>,

    /// Maximum size of a Move user event. Enforced by the VM during execution.
    max_event_emit_size_total: Option<u64>,

    /// Maximum length of a vector in Move. Enforced by the VM during execution, and for constants, by the verifier.
    max_move_vector_len: Option<u64>,

    /// Maximum length of an `Identifier` in Move. Enforced by the bytecode verifier at signing.
    max_move_identifier_len: Option<u64>,

    /// Maximum depth of a Move value within the VM.
    max_move_value_depth: Option<u64>,

    /// Maximum number of variants in an enum. Enforced by the bytecode verifier at signing.
    max_move_enum_variants: Option<u64>,

    /// Maximum number of back edges in Move function. Enforced by the bytecode verifier at signing.
    max_back_edges_per_function: Option<u64>,

    /// Maximum number of back edges in Move module. Enforced by the bytecode verifier at signing.
    max_back_edges_per_module: Option<u64>,

    /// Maximum number of meter `ticks` spent verifying a Move function. Enforced by the bytecode verifier at signing.
    max_verifier_meter_ticks_per_function: Option<u64>,

    /// Maximum number of meter `ticks` spent verifying a Move module. Enforced by the bytecode verifier at signing.
    max_meter_ticks_per_module: Option<u64>,

    /// Maximum number of meter `ticks` spent verifying a Move package. Enforced by the bytecode verifier at signing.
    max_meter_ticks_per_package: Option<u64>,

    // === Object runtime internal operation limits ====
    // These affect dynamic fields
    /// Maximum number of cached objects in the object runtime ObjectStore. Enforced by object runtime during execution
    object_runtime_max_num_cached_objects: Option<u64>,

    /// Maximum number of cached objects in the object runtime ObjectStore in system transaction. Enforced by object runtime during execution
    object_runtime_max_num_cached_objects_system_tx: Option<u64>,

    /// Maximum number of stored objects accessed by object runtime ObjectStore. Enforced by object runtime during execution
    object_runtime_max_num_store_entries: Option<u64>,

    /// Maximum number of stored objects accessed by object runtime ObjectStore in system transaction. Enforced by object runtime during execution
    object_runtime_max_num_store_entries_system_tx: Option<u64>,

    // === Execution gas costs ====
    /// Base cost for any Sui transaction
    base_tx_cost_fixed: Option<u64>,

    /// Additional cost for a transaction that publishes a package
    /// i.e., the base cost of such a transaction is base_tx_cost_fixed + package_publish_cost_fixed
    package_publish_cost_fixed: Option<u64>,

    /// Cost per byte of a Move call transaction
    /// i.e., the cost of such a transaction is base_cost + (base_tx_cost_per_byte * size)
    base_tx_cost_per_byte: Option<u64>,

    /// Cost per byte for a transaction that publishes a package
    package_publish_cost_per_byte: Option<u64>,

    // Per-byte cost of reading an object during transaction execution
    obj_access_cost_read_per_byte: Option<u64>,

    // Per-byte cost of writing an object during transaction execution
    obj_access_cost_mutate_per_byte: Option<u64>,

    // Per-byte cost of deleting an object during transaction execution
    obj_access_cost_delete_per_byte: Option<u64>,

    /// Per-byte cost charged for each input object to a transaction.
    /// Meant to approximate the cost of checking locks for each object
    // TODO: Option<I'm not sure that this cost makes sense. Checking locks is "free"
    // in the sense that an invalid tx that can never be committed/pay gas can
    // force validators to check an arbitrary number of locks. If those checks are
    // "free" for invalid transactions, why charge for them in valid transactions
    // TODO: Option<if we keep this, I think we probably want it to be a fixed cost rather
    // than a per-byte cost. checking an object lock should not require loading an
    // entire object, just consulting an ID -> tx digest map
    obj_access_cost_verify_per_byte: Option<u64>,

    // Maximal nodes which are allowed when converting to a type layout.
    max_type_to_layout_nodes: Option<u64>,

    /// === Gas version. gas model ===

    /// Gas model version, what code we are using to charge gas
    gas_model_version: Option<u64>,

    /// === Storage gas costs ===

    /// Per-byte cost of storing an object in the Sui global object store. Some of this cost may be refundable if the object is later freed
    obj_data_cost_refundable: Option<u64>,

    // Per-byte cost of storing an object in the Sui transaction log (e.g., in CertifiedTransactionEffects)
    // This depends on the size of various fields including the effects
    // TODO: Option<I don't fully understand this^ and more details would be useful
    obj_metadata_cost_non_refundable: Option<u64>,

    /// === Tokenomics ===

    // TODO: Option<this should be changed to u64.
    /// Sender of a txn that touches an object will get this percent of the storage rebate back.
    /// In basis point.
    storage_rebate_rate: Option<u64>,

    /// 5% of the storage fund's share of rewards are reinvested into the storage fund.
    /// In basis point.
    storage_fund_reinvest_rate: Option<u64>,

    /// The share of rewards that will be slashed and redistributed is 50%.
    /// In basis point.
    reward_slashing_rate: Option<u64>,

    /// Unit gas price, Mist per internal gas unit.
    storage_gas_price: Option<u64>,

    /// === Core Protocol ===

    /// Max number of transactions per checkpoint.
    /// Note that this is a protocol constant and not a config as validators must have this set to
    /// the same value, otherwise they *will* fork.
    max_transactions_per_checkpoint: Option<u64>,

    /// Max size of a checkpoint in bytes.
    /// Note that this is a protocol constant and not a config as validators must have this set to
    /// the same value, otherwise they *will* fork.
    max_checkpoint_size_bytes: Option<u64>,

    /// A protocol upgrade always requires 2f+1 stake to agree. We support a buffer of additional
    /// stake (as a fraction of f, expressed in basis points) that is required before an upgrade
    /// can happen automatically. 10000bps would indicate that complete unanimity is required (all
    /// 3f+1 must vote), while 0bps would indicate that 2f+1 is sufficient.
    buffer_stake_for_protocol_upgrade_bps: Option<u64>,

    // === Native Function Costs ===

    // `address` module
    // Cost params for the Move native function `address::from_bytes(bytes: vector<u8>)`
    address_from_bytes_cost_base: Option<u64>,
    // Cost params for the Move native function `address::to_u256(address): u256`
    address_to_u256_cost_base: Option<u64>,
    // Cost params for the Move native function `address::from_u256(u256): address`
    address_from_u256_cost_base: Option<u64>,

    // `config` module
    // Cost params for the Move native function `read_setting_impl<Name: copy + drop + store,
    // SettingValue: key + store, SettingDataValue: store, Value: copy + drop + store,
    // >(config: address, name: address, current_epoch: u64): Option<Value>`
    config_read_setting_impl_cost_base: Option<u64>,
    config_read_setting_impl_cost_per_byte: Option<u64>,

    // `dynamic_field` module
    // Cost params for the Move native function `hash_type_and_key<K: copy + drop + store>(parent: address, k: K): address`
    dynamic_field_hash_type_and_key_cost_base: Option<u64>,
    dynamic_field_hash_type_and_key_type_cost_per_byte: Option<u64>,
    dynamic_field_hash_type_and_key_value_cost_per_byte: Option<u64>,
    dynamic_field_hash_type_and_key_type_tag_cost_per_byte: Option<u64>,
    // Cost params for the Move native function `add_child_object<Child: key>(parent: address, child: Child)`
    dynamic_field_add_child_object_cost_base: Option<u64>,
    dynamic_field_add_child_object_type_cost_per_byte: Option<u64>,
    dynamic_field_add_child_object_value_cost_per_byte: Option<u64>,
    dynamic_field_add_child_object_struct_tag_cost_per_byte: Option<u64>,
    // Cost params for the Move native function `borrow_child_object_mut<Child: key>(parent: &mut UID, id: address): &mut Child`
    dynamic_field_borrow_child_object_cost_base: Option<u64>,
    dynamic_field_borrow_child_object_child_ref_cost_per_byte: Option<u64>,
    dynamic_field_borrow_child_object_type_cost_per_byte: Option<u64>,
    // Cost params for the Move native function `remove_child_object<Child: key>(parent: address, id: address): Child`
    dynamic_field_remove_child_object_cost_base: Option<u64>,
    dynamic_field_remove_child_object_child_cost_per_byte: Option<u64>,
    dynamic_field_remove_child_object_type_cost_per_byte: Option<u64>,
    // Cost params for the Move native function `has_child_object(parent: address, id: address): bool`
    dynamic_field_has_child_object_cost_base: Option<u64>,
    // Cost params for the Move native function `has_child_object_with_ty<Child: key>(parent: address, id: address): bool`
    dynamic_field_has_child_object_with_ty_cost_base: Option<u64>,
    dynamic_field_has_child_object_with_ty_type_cost_per_byte: Option<u64>,
    dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte: Option<u64>,

    // `event` module
    // Cost params for the Move native function `event::emit<T: copy + drop>(event: T)`
    event_emit_cost_base: Option<u64>,
    event_emit_value_size_derivation_cost_per_byte: Option<u64>,
    event_emit_tag_size_derivation_cost_per_byte: Option<u64>,
    event_emit_output_cost_per_byte: Option<u64>,

    //  `object` module
    // Cost params for the Move native function `borrow_uid<T: key>(obj: &T): &UID`
    object_borrow_uid_cost_base: Option<u64>,
    // Cost params for the Move native function `delete_impl(id: address)`
    object_delete_impl_cost_base: Option<u64>,
    // Cost params for the Move native function `record_new_uid(id: address)`
    object_record_new_uid_cost_base: Option<u64>,

    // Transfer
    // Cost params for the Move native function `transfer_impl<T: key>(obj: T, recipient: address)`
    transfer_transfer_internal_cost_base: Option<u64>,
    // Cost params for the Move native function `freeze_object<T: key>(obj: T)`
    transfer_freeze_object_cost_base: Option<u64>,
    // Cost params for the Move native function `share_object<T: key>(obj: T)`
    transfer_share_object_cost_base: Option<u64>,
    // Cost params for the Move native function
    // `receive_object<T: key>(p: &mut UID, recv: Receiving<T>T)`
    transfer_receive_object_cost_base: Option<u64>,

    // TxContext
    // Cost params for the Move native function `transfer_impl<T: key>(obj: T, recipient: address)`
    tx_context_derive_id_cost_base: Option<u64>,

    // Types
    // Cost params for the Move native function `is_one_time_witness<T: drop>(_: &T): bool`
    types_is_one_time_witness_cost_base: Option<u64>,
    types_is_one_time_witness_type_tag_cost_per_byte: Option<u64>,
    types_is_one_time_witness_type_cost_per_byte: Option<u64>,

    // Validator
    // Cost params for the Move native function `validate_metadata_bcs(metadata: vector<u8>)`
    validator_validate_metadata_cost_base: Option<u64>,
    validator_validate_metadata_data_cost_per_byte: Option<u64>,

    // Crypto natives
    crypto_invalid_arguments_cost: Option<u64>,
    // bls12381::bls12381_min_sig_verify
    bls12381_bls12381_min_sig_verify_cost_base: Option<u64>,
    bls12381_bls12381_min_sig_verify_msg_cost_per_byte: Option<u64>,
    bls12381_bls12381_min_sig_verify_msg_cost_per_block: Option<u64>,

    // bls12381::bls12381_min_pk_verify
    bls12381_bls12381_min_pk_verify_cost_base: Option<u64>,
    bls12381_bls12381_min_pk_verify_msg_cost_per_byte: Option<u64>,
    bls12381_bls12381_min_pk_verify_msg_cost_per_block: Option<u64>,

    // ecdsa_k1::ecrecover
    ecdsa_k1_ecrecover_keccak256_cost_base: Option<u64>,
    ecdsa_k1_ecrecover_keccak256_msg_cost_per_byte: Option<u64>,
    ecdsa_k1_ecrecover_keccak256_msg_cost_per_block: Option<u64>,
    ecdsa_k1_ecrecover_sha256_cost_base: Option<u64>,
    ecdsa_k1_ecrecover_sha256_msg_cost_per_byte: Option<u64>,
    ecdsa_k1_ecrecover_sha256_msg_cost_per_block: Option<u64>,

    // ecdsa_k1::decompress_pubkey
    ecdsa_k1_decompress_pubkey_cost_base: Option<u64>,

    // ecdsa_k1::secp256k1_verify
    ecdsa_k1_secp256k1_verify_keccak256_cost_base: Option<u64>,
    ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_byte: Option<u64>,
    ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_block: Option<u64>,
    ecdsa_k1_secp256k1_verify_sha256_cost_base: Option<u64>,
    ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_byte: Option<u64>,
    ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_block: Option<u64>,

    // ecdsa_r1::ecrecover
    ecdsa_r1_ecrecover_keccak256_cost_base: Option<u64>,
    ecdsa_r1_ecrecover_keccak256_msg_cost_per_byte: Option<u64>,
    ecdsa_r1_ecrecover_keccak256_msg_cost_per_block: Option<u64>,
    ecdsa_r1_ecrecover_sha256_cost_base: Option<u64>,
    ecdsa_r1_ecrecover_sha256_msg_cost_per_byte: Option<u64>,
    ecdsa_r1_ecrecover_sha256_msg_cost_per_block: Option<u64>,

    // ecdsa_r1::secp256k1_verify
    ecdsa_r1_secp256r1_verify_keccak256_cost_base: Option<u64>,
    ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_byte: Option<u64>,
    ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_block: Option<u64>,
    ecdsa_r1_secp256r1_verify_sha256_cost_base: Option<u64>,
    ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_byte: Option<u64>,
    ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_block: Option<u64>,

    // ecvrf::verify
    ecvrf_ecvrf_verify_cost_base: Option<u64>,
    ecvrf_ecvrf_verify_alpha_string_cost_per_byte: Option<u64>,
    ecvrf_ecvrf_verify_alpha_string_cost_per_block: Option<u64>,

    // ed25519
    ed25519_ed25519_verify_cost_base: Option<u64>,
    ed25519_ed25519_verify_msg_cost_per_byte: Option<u64>,
    ed25519_ed25519_verify_msg_cost_per_block: Option<u64>,

    // groth16::prepare_verifying_key
    groth16_prepare_verifying_key_bls12381_cost_base: Option<u64>,
    groth16_prepare_verifying_key_bn254_cost_base: Option<u64>,

    // groth16::verify_groth16_proof_internal
    groth16_verify_groth16_proof_internal_bls12381_cost_base: Option<u64>,
    groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input: Option<u64>,
    groth16_verify_groth16_proof_internal_bn254_cost_base: Option<u64>,
    groth16_verify_groth16_proof_internal_bn254_cost_per_public_input: Option<u64>,
    groth16_verify_groth16_proof_internal_public_input_cost_per_byte: Option<u64>,

    // hash::blake2b256
    hash_blake2b256_cost_base: Option<u64>,
    hash_blake2b256_data_cost_per_byte: Option<u64>,
    hash_blake2b256_data_cost_per_block: Option<u64>,

    // hash::keccak256
    hash_keccak256_cost_base: Option<u64>,
    hash_keccak256_data_cost_per_byte: Option<u64>,
    hash_keccak256_data_cost_per_block: Option<u64>,

    // poseidon::poseidon_bn254
    poseidon_bn254_cost_base: Option<u64>,
    poseidon_bn254_cost_per_block: Option<u64>,

    // group_ops
    group_ops_bls12381_decode_scalar_cost: Option<u64>,
    group_ops_bls12381_decode_g1_cost: Option<u64>,
    group_ops_bls12381_decode_g2_cost: Option<u64>,
    group_ops_bls12381_decode_gt_cost: Option<u64>,
    group_ops_bls12381_scalar_add_cost: Option<u64>,
    group_ops_bls12381_g1_add_cost: Option<u64>,
    group_ops_bls12381_g2_add_cost: Option<u64>,
    group_ops_bls12381_gt_add_cost: Option<u64>,
    group_ops_bls12381_scalar_sub_cost: Option<u64>,
    group_ops_bls12381_g1_sub_cost: Option<u64>,
    group_ops_bls12381_g2_sub_cost: Option<u64>,
    group_ops_bls12381_gt_sub_cost: Option<u64>,
    group_ops_bls12381_scalar_mul_cost: Option<u64>,
    group_ops_bls12381_g1_mul_cost: Option<u64>,
    group_ops_bls12381_g2_mul_cost: Option<u64>,
    group_ops_bls12381_gt_mul_cost: Option<u64>,
    group_ops_bls12381_scalar_div_cost: Option<u64>,
    group_ops_bls12381_g1_div_cost: Option<u64>,
    group_ops_bls12381_g2_div_cost: Option<u64>,
    group_ops_bls12381_gt_div_cost: Option<u64>,
    group_ops_bls12381_g1_hash_to_base_cost: Option<u64>,
    group_ops_bls12381_g2_hash_to_base_cost: Option<u64>,
    group_ops_bls12381_g1_hash_to_cost_per_byte: Option<u64>,
    group_ops_bls12381_g2_hash_to_cost_per_byte: Option<u64>,
    group_ops_bls12381_g1_msm_base_cost: Option<u64>,
    group_ops_bls12381_g2_msm_base_cost: Option<u64>,
    group_ops_bls12381_g1_msm_base_cost_per_input: Option<u64>,
    group_ops_bls12381_g2_msm_base_cost_per_input: Option<u64>,
    group_ops_bls12381_msm_max_len: Option<u32>,
    group_ops_bls12381_pairing_cost: Option<u64>,
    group_ops_bls12381_g1_to_uncompressed_g1_cost: Option<u64>,
    group_ops_bls12381_uncompressed_g1_to_g1_cost: Option<u64>,
    group_ops_bls12381_uncompressed_g1_sum_base_cost: Option<u64>,
    group_ops_bls12381_uncompressed_g1_sum_cost_per_term: Option<u64>,
    group_ops_bls12381_uncompressed_g1_sum_max_terms: Option<u64>,

    // hmac::hmac_sha3_256
    hmac_hmac_sha3_256_cost_base: Option<u64>,
    hmac_hmac_sha3_256_input_cost_per_byte: Option<u64>,
    hmac_hmac_sha3_256_input_cost_per_block: Option<u64>,

    // zklogin::check_zklogin_id
    check_zklogin_id_cost_base: Option<u64>,
    // zklogin::check_zklogin_issuer
    check_zklogin_issuer_cost_base: Option<u64>,

    vdf_verify_vdf_cost: Option<u64>,
    vdf_hash_to_input_cost: Option<u64>,

    // nitro_attestation::load_nitro_attestation
    nitro_attestation_parse_base_cost: Option<u64>,
    nitro_attestation_parse_cost_per_byte: Option<u64>,
    nitro_attestation_verify_base_cost: Option<u64>,
    nitro_attestation_verify_cost_per_cert: Option<u64>,

    // Stdlib costs
    bcs_per_byte_serialized_cost: Option<u64>,
    bcs_legacy_min_output_size_cost: Option<u64>,
    bcs_failure_cost: Option<u64>,

    hash_sha2_256_base_cost: Option<u64>,
    hash_sha2_256_per_byte_cost: Option<u64>,
    hash_sha2_256_legacy_min_input_len_cost: Option<u64>,
    hash_sha3_256_base_cost: Option<u64>,
    hash_sha3_256_per_byte_cost: Option<u64>,
    hash_sha3_256_legacy_min_input_len_cost: Option<u64>,
    type_name_get_base_cost: Option<u64>,
    type_name_get_per_byte_cost: Option<u64>,

    string_check_utf8_base_cost: Option<u64>,
    string_check_utf8_per_byte_cost: Option<u64>,
    string_is_char_boundary_base_cost: Option<u64>,
    string_sub_string_base_cost: Option<u64>,
    string_sub_string_per_byte_cost: Option<u64>,
    string_index_of_base_cost: Option<u64>,
    string_index_of_per_byte_pattern_cost: Option<u64>,
    string_index_of_per_byte_searched_cost: Option<u64>,

    vector_empty_base_cost: Option<u64>,
    vector_length_base_cost: Option<u64>,
    vector_push_back_base_cost: Option<u64>,
    vector_push_back_legacy_per_abstract_memory_unit_cost: Option<u64>,
    vector_borrow_base_cost: Option<u64>,
    vector_pop_back_base_cost: Option<u64>,
    vector_destroy_empty_base_cost: Option<u64>,
    vector_swap_base_cost: Option<u64>,
    debug_print_base_cost: Option<u64>,
    debug_print_stack_trace_base_cost: Option<u64>,

    // ==== Ephemeral (consensus only) params deleted ====
    //
    // Const params for consensus scoring decision
    // The scaling factor property for the MED outlier detection
    // scoring_decision_mad_divisor: Option<f64>,
    // The cutoff value for the MED outlier detection
    // scoring_decision_cutoff_value: Option<f64>,
    /// === Execution Version ===
    execution_version: Option<u64>,

    // Dictates the threshold (percentage of stake) that is used to calculate the "bad" nodes to be
    // swapped when creating the consensus schedule. The values should be of the range [0 - 33]. Anything
    // above 33 (f) will not be allowed.
    consensus_bad_nodes_stake_threshold: Option<u64>,

    max_jwk_votes_per_validator_per_epoch: Option<u64>,
    // The maximum age of a JWK in epochs before it is removed from the AuthenticatorState object.
    // Applied at the end of an epoch as a delta from the new epoch value, so setting this to 1
    // will cause the new epoch to start with JWKs from the previous epoch still valid.
    max_age_of_jwk_in_epochs: Option<u64>,

    /// === random beacon ===

    /// Maximum allowed precision loss when reducing voting weights for the random beacon
    /// protocol.
    random_beacon_reduction_allowed_delta: Option<u16>,

    /// Minimum number of shares below which voting weights will not be reduced for the
    /// random beacon protocol.
    random_beacon_reduction_lower_bound: Option<u32>,

    /// Consensus Round after which DKG should be aborted and randomness disabled for
    /// the epoch, if it hasn't already completed.
    random_beacon_dkg_timeout_round: Option<u32>,

    /// Minimum interval between consecutive rounds of generated randomness.
    random_beacon_min_round_interval_ms: Option<u64>,

    /// Version of the random beacon DKG protocol.
    /// 0 was deprecated (and currently not supported), 1 is the default version.
    random_beacon_dkg_version: Option<u64>,

    /// The maximum serialised transaction size (in bytes) accepted by consensus. That should be bigger than the
    /// `max_tx_size_bytes` with some additional headroom.
    consensus_max_transaction_size_bytes: Option<u64>,
    /// The maximum size of transactions included in a consensus block.
    consensus_max_transactions_in_block_bytes: Option<u64>,
    /// The maximum number of transactions included in a consensus block.
    consensus_max_num_transactions_in_block: Option<u64>,

    /// The maximum number of rounds where transaction voting is allowed.
    consensus_voting_rounds: Option<u32>,

    /// DEPRECATED. Do not use.
    max_accumulated_txn_cost_per_object_in_narwhal_commit: Option<u64>,

    /// The max number of consensus rounds a transaction can be deferred due to shared object congestion.
    /// Transactions will be cancelled after this many rounds.
    max_deferral_rounds_for_congestion_control: Option<u64>,

    /// If >0, congestion control will allow the configured maximum accumulated cost per object
    /// to be exceeded by at most the given amount. Only one limit-exceeding transaction per
    /// object will be allowed, unless bursting is configured below.
    max_txn_cost_overage_per_object_in_commit: Option<u64>,

    /// If >0, congestion control will allow transactions in total cost equaling the
    /// configured amount to exceed the configured maximum accumulated cost per object.
    /// As above, up to one transaction per object exceeding the burst limit will be allowed.
    allowed_txn_cost_overage_burst_per_object_in_commit: Option<u64>,

    /// Minimum interval of commit timestamps between consecutive checkpoints.
    min_checkpoint_interval_ms: Option<u64>,

    /// Version number to use for version_specific_data in `CheckpointSummary`.
    checkpoint_summary_version_specific_data: Option<u64>,

    /// The max number of transactions that can be included in a single Soft Bundle.
    max_soft_bundle_size: Option<u64>,

    /// Whether to try to form bridge committee
    // Note: this is not a feature flag because we want to distinguish between
    // `None` and `Some(false)`, as committee was already finalized on Testnet.
    bridge_should_try_to_finalize_committee: Option<bool>,

    /// The max accumulated txn execution cost per object in a mysticeti. Transactions
    /// in a commit will be deferred once their touch shared objects hit this limit,
    /// unless the selected congestion control mode allows overage.
    /// This config plays the same role as `max_accumulated_txn_cost_per_object_in_narwhal_commit`
    /// but for mysticeti commits due to that mysticeti has higher commit rate.
    max_accumulated_txn_cost_per_object_in_mysticeti_commit: Option<u64>,

    /// As above, but separate per-commit budget for transactions that use randomness.
    /// If not configured, uses the setting for `max_accumulated_txn_cost_per_object_in_mysticeti_commit`.
    max_accumulated_randomness_txn_cost_per_object_in_mysticeti_commit: Option<u64>,

    /// Configures the garbage collection depth for consensus. When is unset or `0` then the garbage collection
    /// is disabled.
    consensus_gc_depth: Option<u32>,

    /// Used to calculate the max transaction cost when using TotalGasBudgetWithCap as shard
    /// object congestion control strategy. Basically the max transaction cost is calculated as
    /// (num of input object + num of commands) * this factor.
    gas_budget_based_txn_cost_cap_factor: Option<u64>,

    /// Adds an absolute cap on the maximum transaction cost when using TotalGasBudgetWithCap at
    /// the given multiple of the per-commit budget.
    gas_budget_based_txn_cost_absolute_cap_commit_count: Option<u64>,

    /// SIP-45: K in the formula `amplification_factor = max(0, gas_price / reference_gas_price - K)`.
    /// This is the threshold for activating consensus amplification.
    sip_45_consensus_amplification_threshold: Option<u64>,

    /// Enables use of v2 of the object per-epoch marker table with FullObjectID keys.
    use_object_per_epoch_marker_table_v2: Option<bool>,
}

// feature flags
impl ProtocolConfig {
    // Add checks for feature flag support here, e.g.:
    // pub fn check_new_protocol_feature_supported(&self) -> Result<(), Error> {
    //     if self.feature_flags.new_protocol_feature_supported {
    //         Ok(())
    //     } else {
    //         Err(Error(format!(
    //             "new_protocol_feature is not supported at {:?}",
    //             self.version
    //         )))
    //     }
    // }

    pub fn check_package_upgrades_supported(&self) -> Result<(), Error> {
        if self.feature_flags.package_upgrades {
            Ok(())
        } else {
            Err(Error(format!(
                "package upgrades are not supported at {:?}",
                self.version
            )))
        }
    }

    pub fn allow_receiving_object_id(&self) -> bool {
        self.feature_flags.allow_receiving_object_id
    }

    pub fn receiving_objects_supported(&self) -> bool {
        self.feature_flags.receive_objects
    }

    pub fn package_upgrades_supported(&self) -> bool {
        self.feature_flags.package_upgrades
    }

    pub fn check_commit_root_state_digest_supported(&self) -> bool {
        self.feature_flags.commit_root_state_digest
    }

    pub fn get_advance_epoch_start_time_in_safe_mode(&self) -> bool {
        self.feature_flags.advance_epoch_start_time_in_safe_mode
    }

    pub fn loaded_child_objects_fixed(&self) -> bool {
        self.feature_flags.loaded_child_objects_fixed
    }

    pub fn missing_type_is_compatibility_error(&self) -> bool {
        self.feature_flags.missing_type_is_compatibility_error
    }

    pub fn scoring_decision_with_validity_cutoff(&self) -> bool {
        self.feature_flags.scoring_decision_with_validity_cutoff
    }

    pub fn narwhal_versioned_metadata(&self) -> bool {
        self.feature_flags.narwhal_versioned_metadata
    }

    pub fn consensus_order_end_of_epoch_last(&self) -> bool {
        self.feature_flags.consensus_order_end_of_epoch_last
    }

    pub fn disallow_adding_abilities_on_upgrade(&self) -> bool {
        self.feature_flags.disallow_adding_abilities_on_upgrade
    }

    pub fn disable_invariant_violation_check_in_swap_loc(&self) -> bool {
        self.feature_flags
            .disable_invariant_violation_check_in_swap_loc
    }

    pub fn advance_to_highest_supported_protocol_version(&self) -> bool {
        self.feature_flags
            .advance_to_highest_supported_protocol_version
    }

    pub fn ban_entry_init(&self) -> bool {
        self.feature_flags.ban_entry_init
    }

    pub fn package_digest_hash_module(&self) -> bool {
        self.feature_flags.package_digest_hash_module
    }

    pub fn disallow_change_struct_type_params_on_upgrade(&self) -> bool {
        self.feature_flags
            .disallow_change_struct_type_params_on_upgrade
    }

    pub fn no_extraneous_module_bytes(&self) -> bool {
        self.feature_flags.no_extraneous_module_bytes
    }

    pub fn zklogin_auth(&self) -> bool {
        self.feature_flags.zklogin_auth
    }

    pub fn zklogin_supported_providers(&self) -> &BTreeSet<String> {
        &self.feature_flags.zklogin_supported_providers
    }

    pub fn consensus_transaction_ordering(&self) -> ConsensusTransactionOrdering {
        self.feature_flags.consensus_transaction_ordering
    }

    pub fn simplified_unwrap_then_delete(&self) -> bool {
        self.feature_flags.simplified_unwrap_then_delete
    }

    pub fn supports_upgraded_multisig(&self) -> bool {
        self.feature_flags.upgraded_multisig_supported
    }

    pub fn txn_base_cost_as_multiplier(&self) -> bool {
        self.feature_flags.txn_base_cost_as_multiplier
    }

    pub fn shared_object_deletion(&self) -> bool {
        self.feature_flags.shared_object_deletion
    }

    pub fn narwhal_new_leader_election_schedule(&self) -> bool {
        self.feature_flags.narwhal_new_leader_election_schedule
    }

    pub fn loaded_child_object_format(&self) -> bool {
        self.feature_flags.loaded_child_object_format
    }

    pub fn enable_jwk_consensus_updates(&self) -> bool {
        let ret = self.feature_flags.enable_jwk_consensus_updates;
        if ret {
            // jwk updates required end-of-epoch transactions
            assert!(self.feature_flags.end_of_epoch_transaction_supported);
        }
        ret
    }

    pub fn simple_conservation_checks(&self) -> bool {
        self.feature_flags.simple_conservation_checks
    }

    pub fn loaded_child_object_format_type(&self) -> bool {
        self.feature_flags.loaded_child_object_format_type
    }

    pub fn end_of_epoch_transaction_supported(&self) -> bool {
        let ret = self.feature_flags.end_of_epoch_transaction_supported;
        if !ret {
            // jwk updates required end-of-epoch transactions
            assert!(!self.feature_flags.enable_jwk_consensus_updates);
        }
        ret
    }

    pub fn recompute_has_public_transfer_in_execution(&self) -> bool {
        self.feature_flags
            .recompute_has_public_transfer_in_execution
    }

    // this function only exists for readability in the genesis code.
    pub fn create_authenticator_state_in_genesis(&self) -> bool {
        self.enable_jwk_consensus_updates()
    }

    pub fn random_beacon(&self) -> bool {
        self.feature_flags.random_beacon
    }

    pub fn dkg_version(&self) -> u64 {
        // Version 0 was deprecated and removed, the default is 1 if not set.
        self.random_beacon_dkg_version.unwrap_or(1)
    }

    pub fn enable_bridge(&self) -> bool {
        let ret = self.feature_flags.bridge;
        if ret {
            // bridge required end-of-epoch transactions
            assert!(self.feature_flags.end_of_epoch_transaction_supported);
        }
        ret
    }

    pub fn should_try_to_finalize_bridge_committee(&self) -> bool {
        if !self.enable_bridge() {
            return false;
        }
        // In the older protocol version, always try to finalize the committee.
        self.bridge_should_try_to_finalize_committee.unwrap_or(true)
    }

    pub fn enable_effects_v2(&self) -> bool {
        self.feature_flags.enable_effects_v2
    }

    pub fn narwhal_certificate_v2(&self) -> bool {
        self.feature_flags.narwhal_certificate_v2
    }

    pub fn verify_legacy_zklogin_address(&self) -> bool {
        self.feature_flags.verify_legacy_zklogin_address
    }

    pub fn accept_zklogin_in_multisig(&self) -> bool {
        self.feature_flags.accept_zklogin_in_multisig
    }

    pub fn zklogin_max_epoch_upper_bound_delta(&self) -> Option<u64> {
        self.feature_flags.zklogin_max_epoch_upper_bound_delta
    }

    pub fn throughput_aware_consensus_submission(&self) -> bool {
        self.feature_flags.throughput_aware_consensus_submission
    }

    pub fn include_consensus_digest_in_prologue(&self) -> bool {
        self.feature_flags.include_consensus_digest_in_prologue
    }

    pub fn record_consensus_determined_version_assignments_in_prologue(&self) -> bool {
        self.feature_flags
            .record_consensus_determined_version_assignments_in_prologue
    }

    pub fn record_consensus_determined_version_assignments_in_prologue_v2(&self) -> bool {
        self.feature_flags
            .record_consensus_determined_version_assignments_in_prologue_v2
    }

    pub fn prepend_prologue_tx_in_consensus_commit_in_checkpoints(&self) -> bool {
        self.feature_flags
            .prepend_prologue_tx_in_consensus_commit_in_checkpoints
    }

    pub fn hardened_otw_check(&self) -> bool {
        self.feature_flags.hardened_otw_check
    }

    pub fn enable_poseidon(&self) -> bool {
        self.feature_flags.enable_poseidon
    }

    pub fn enable_coin_deny_list_v1(&self) -> bool {
        self.feature_flags.enable_coin_deny_list
    }

    pub fn enable_coin_deny_list_v2(&self) -> bool {
        self.feature_flags.enable_coin_deny_list_v2
    }

    pub fn enable_group_ops_native_functions(&self) -> bool {
        self.feature_flags.enable_group_ops_native_functions
    }

    pub fn enable_group_ops_native_function_msm(&self) -> bool {
        self.feature_flags.enable_group_ops_native_function_msm
    }

    pub fn reject_mutable_random_on_entry_functions(&self) -> bool {
        self.feature_flags.reject_mutable_random_on_entry_functions
    }

    pub fn per_object_congestion_control_mode(&self) -> PerObjectCongestionControlMode {
        self.feature_flags.per_object_congestion_control_mode
    }

    pub fn consensus_choice(&self) -> ConsensusChoice {
        self.feature_flags.consensus_choice
    }

    pub fn consensus_network(&self) -> ConsensusNetwork {
        self.feature_flags.consensus_network
    }

    pub fn reshare_at_same_initial_version(&self) -> bool {
        self.feature_flags.reshare_at_same_initial_version
    }

    pub fn resolve_abort_locations_to_package_id(&self) -> bool {
        self.feature_flags.resolve_abort_locations_to_package_id
    }

    pub fn mysticeti_use_committed_subdag_digest(&self) -> bool {
        self.feature_flags.mysticeti_use_committed_subdag_digest
    }

    pub fn enable_vdf(&self) -> bool {
        self.feature_flags.enable_vdf
    }

    pub fn fresh_vm_on_framework_upgrade(&self) -> bool {
        self.feature_flags.fresh_vm_on_framework_upgrade
    }

    pub fn mysticeti_num_leaders_per_round(&self) -> Option<usize> {
        self.feature_flags.mysticeti_num_leaders_per_round
    }

    pub fn soft_bundle(&self) -> bool {
        self.feature_flags.soft_bundle
    }

    pub fn passkey_auth(&self) -> bool {
        self.feature_flags.passkey_auth
    }

    pub fn authority_capabilities_v2(&self) -> bool {
        self.feature_flags.authority_capabilities_v2
    }

    pub fn max_transaction_size_bytes(&self) -> u64 {
        // Provide a default value if protocol config version is too low.
        self.consensus_max_transaction_size_bytes
            .unwrap_or(256 * 1024)
    }

    pub fn max_transactions_in_block_bytes(&self) -> u64 {
        if cfg!(msim) {
            256 * 1024
        } else {
            self.consensus_max_transactions_in_block_bytes
                .unwrap_or(512 * 1024)
        }
    }

    pub fn max_num_transactions_in_block(&self) -> u64 {
        if cfg!(msim) {
            8
        } else {
            self.consensus_max_num_transactions_in_block.unwrap_or(512)
        }
    }

    pub fn rethrow_serialization_type_layout_errors(&self) -> bool {
        self.feature_flags.rethrow_serialization_type_layout_errors
    }

    pub fn consensus_distributed_vote_scoring_strategy(&self) -> bool {
        self.feature_flags
            .consensus_distributed_vote_scoring_strategy
    }

    pub fn consensus_round_prober(&self) -> bool {
        self.feature_flags.consensus_round_prober
    }

    pub fn validate_identifier_inputs(&self) -> bool {
        self.feature_flags.validate_identifier_inputs
    }

    pub fn gc_depth(&self) -> u32 {
        if cfg!(msim) {
            // exercise a very low gc_depth
            5
        } else {
            self.consensus_gc_depth.unwrap_or(0)
        }
    }

    pub fn mysticeti_fastpath(&self) -> bool {
        if let Some(enabled) = is_mysticeti_fpc_enabled_in_env() {
            return enabled;
        }
        self.feature_flags.mysticeti_fastpath
    }

    pub fn relocate_event_module(&self) -> bool {
        self.feature_flags.relocate_event_module
    }

    pub fn uncompressed_g1_group_elements(&self) -> bool {
        self.feature_flags.uncompressed_g1_group_elements
    }

    pub fn disallow_new_modules_in_deps_only_packages(&self) -> bool {
        self.feature_flags
            .disallow_new_modules_in_deps_only_packages
    }

    pub fn consensus_smart_ancestor_selection(&self) -> bool {
        self.feature_flags.consensus_smart_ancestor_selection
    }

    pub fn consensus_round_prober_probe_accepted_rounds(&self) -> bool {
        self.feature_flags
            .consensus_round_prober_probe_accepted_rounds
    }

    pub fn native_charging_v2(&self) -> bool {
        self.feature_flags.native_charging_v2
    }

    pub fn consensus_linearize_subdag_v2(&self) -> bool {
        let res = self.feature_flags.consensus_linearize_subdag_v2;
        assert!(
            !res || self.gc_depth() > 0,
            "The consensus linearize sub dag V2 requires GC to be enabled"
        );
        res
    }

    pub fn convert_type_argument_error(&self) -> bool {
        self.feature_flags.convert_type_argument_error
    }

    pub fn variant_nodes(&self) -> bool {
        self.feature_flags.variant_nodes
    }

    pub fn consensus_zstd_compression(&self) -> bool {
        self.feature_flags.consensus_zstd_compression
    }
    pub fn enable_nitro_attestation(&self) -> bool {
        self.feature_flags.enable_nitro_attestation
    }
}

#[cfg(not(msim))]
static POISON_VERSION_METHODS: AtomicBool = AtomicBool::new(false);

// Use a thread local in sim tests for test isolation.
#[cfg(msim)]
thread_local! {
    static POISON_VERSION_METHODS: AtomicBool = AtomicBool::new(false);
}

// Instantiations for each protocol version.
impl ProtocolConfig {
    /// Get the value ProtocolConfig that are in effect during the given protocol version.
    pub fn get_for_version(version: ProtocolVersion, chain: Chain) -> Self {
        // ProtocolVersion can be deserialized so we need to check it here as well.
        assert!(
            version >= ProtocolVersion::MIN,
            "Network protocol version is {:?}, but the minimum supported version by the binary is {:?}. Please upgrade the binary.",
            version,
            ProtocolVersion::MIN.0,
        );
        assert!(
            version <= ProtocolVersion::MAX_ALLOWED,
            "Network protocol version is {:?}, but the maximum supported version by the binary is {:?}. Please upgrade the binary.",
            version,
            ProtocolVersion::MAX_ALLOWED.0,
        );

        let mut ret = Self::get_for_version_impl(version, chain);
        ret.version = version;

        ret = CONFIG_OVERRIDE.with(|ovr| {
            if let Some(override_fn) = &*ovr.borrow() {
                warn!(
                    "overriding ProtocolConfig settings with custom settings (you should not see this log outside of tests)"
                );
                override_fn(version, ret)
            } else {
                ret
            }
        });

        if std::env::var("SUI_PROTOCOL_CONFIG_OVERRIDE_ENABLE").is_ok() {
            warn!("overriding ProtocolConfig settings with custom settings; this may break non-local networks");
            let overrides: ProtocolConfigOptional =
                serde_env::from_env_with_prefix("SUI_PROTOCOL_CONFIG_OVERRIDE")
                    .expect("failed to parse ProtocolConfig override env variables");
            overrides.apply_to(&mut ret);
        }

        ret
    }

    /// Get the value ProtocolConfig that are in effect during the given protocol version.
    /// Or none if the version is not supported.
    pub fn get_for_version_if_supported(version: ProtocolVersion, chain: Chain) -> Option<Self> {
        if version.0 >= ProtocolVersion::MIN.0 && version.0 <= ProtocolVersion::MAX_ALLOWED.0 {
            let mut ret = Self::get_for_version_impl(version, chain);
            ret.version = version;
            Some(ret)
        } else {
            None
        }
    }

    #[cfg(not(msim))]
    pub fn poison_get_for_min_version() {
        POISON_VERSION_METHODS.store(true, Ordering::Relaxed);
    }

    #[cfg(not(msim))]
    fn load_poison_get_for_min_version() -> bool {
        POISON_VERSION_METHODS.load(Ordering::Relaxed)
    }

    #[cfg(msim)]
    pub fn poison_get_for_min_version() {
        POISON_VERSION_METHODS.with(|p| p.store(true, Ordering::Relaxed));
    }

    #[cfg(msim)]
    fn load_poison_get_for_min_version() -> bool {
        POISON_VERSION_METHODS.with(|p| p.load(Ordering::Relaxed))
    }

    /// Convenience to get the constants at the current minimum supported version.
    /// Mainly used by client code that may not yet be protocol-version aware.
    pub fn get_for_min_version() -> Self {
        if Self::load_poison_get_for_min_version() {
            panic!("get_for_min_version called on validator");
        }
        ProtocolConfig::get_for_version(ProtocolVersion::MIN, Chain::Unknown)
    }

    /// CAREFUL! - You probably want to use `get_for_version` instead.
    ///
    /// Convenience to get the constants at the current maximum supported version.
    /// Mainly used by genesis. Note well that this function uses the max version
    /// supported locally by the node, which is not necessarily the current version
    /// of the network. ALSO, this function disregards chain specific config (by
    /// using Chain::Unknown), thereby potentially returning a protocol config that
    /// is incorrect for some feature flags. Definitely safe for testing and for
    /// protocol version 11 and prior.
    #[allow(non_snake_case)]
    pub fn get_for_max_version_UNSAFE() -> Self {
        if Self::load_poison_get_for_min_version() {
            panic!("get_for_max_version_UNSAFE called on validator");
        }
        ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Unknown)
    }

    fn get_for_version_impl(version: ProtocolVersion, chain: Chain) -> Self {
        #[cfg(msim)]
        {
            // populate the fake simulator version # with a different base tx cost.
            if version == ProtocolVersion::MAX_ALLOWED {
                let mut config = Self::get_for_version_impl(version - 1, Chain::Unknown);
                config.base_tx_cost_fixed = Some(config.base_tx_cost_fixed() + 1000);
                return config;
            }
        }

        // IMPORTANT: Never modify the value of any constant for a pre-existing protocol version.
        // To change the values here you must create a new protocol version with the new values!
        let mut cfg = Self {
            // will be overwritten before being returned
            version,

            // All flags are disabled in V1
            feature_flags: Default::default(),

            max_tx_size_bytes: Some(128 * 1024),
            // We need this number to be at least 100x less than `max_serialized_tx_effects_size_bytes`otherwise effects can be huge
            max_input_objects: Some(2048),
            max_serialized_tx_effects_size_bytes: Some(512 * 1024),
            max_serialized_tx_effects_size_bytes_system_tx: Some(512 * 1024 * 16),
            max_gas_payment_objects: Some(256),
            max_modules_in_publish: Some(128),
            max_package_dependencies: None,
            max_arguments: Some(512),
            max_type_arguments: Some(16),
            max_type_argument_depth: Some(16),
            max_pure_argument_size: Some(16 * 1024),
            max_programmable_tx_commands: Some(1024),
            move_binary_format_version: Some(6),
            min_move_binary_format_version: None,
            binary_module_handles: None,
            binary_struct_handles: None,
            binary_function_handles: None,
            binary_function_instantiations: None,
            binary_signatures: None,
            binary_constant_pool: None,
            binary_identifiers: None,
            binary_address_identifiers: None,
            binary_struct_defs: None,
            binary_struct_def_instantiations: None,
            binary_function_defs: None,
            binary_field_handles: None,
            binary_field_instantiations: None,
            binary_friend_decls: None,
            binary_enum_defs: None,
            binary_enum_def_instantiations: None,
            binary_variant_handles: None,
            binary_variant_instantiation_handles: None,
            max_move_object_size: Some(250 * 1024),
            max_move_package_size: Some(100 * 1024),
            max_publish_or_upgrade_per_ptb: None,
            max_tx_gas: Some(10_000_000_000),
            max_gas_price: Some(100_000),
            max_gas_computation_bucket: Some(5_000_000),
            max_loop_depth: Some(5),
            max_generic_instantiation_length: Some(32),
            max_function_parameters: Some(128),
            max_basic_blocks: Some(1024),
            max_value_stack_size: Some(1024),
            max_type_nodes: Some(256),
            max_push_size: Some(10000),
            max_struct_definitions: Some(200),
            max_function_definitions: Some(1000),
            max_fields_in_struct: Some(32),
            max_dependency_depth: Some(100),
            max_num_event_emit: Some(256),
            max_num_new_move_object_ids: Some(2048),
            max_num_new_move_object_ids_system_tx: Some(2048 * 16),
            max_num_deleted_move_object_ids: Some(2048),
            max_num_deleted_move_object_ids_system_tx: Some(2048 * 16),
            max_num_transferred_move_object_ids: Some(2048),
            max_num_transferred_move_object_ids_system_tx: Some(2048 * 16),
            max_event_emit_size: Some(250 * 1024),
            max_move_vector_len: Some(256 * 1024),
            max_type_to_layout_nodes: None,

            max_back_edges_per_function: Some(10_000),
            max_back_edges_per_module: Some(10_000),
            max_verifier_meter_ticks_per_function: Some(6_000_000),
            max_meter_ticks_per_module: Some(6_000_000),
            max_meter_ticks_per_package: None,

            object_runtime_max_num_cached_objects: Some(1000),
            object_runtime_max_num_cached_objects_system_tx: Some(1000 * 16),
            object_runtime_max_num_store_entries: Some(1000),
            object_runtime_max_num_store_entries_system_tx: Some(1000 * 16),
            base_tx_cost_fixed: Some(110_000),
            package_publish_cost_fixed: Some(1_000),
            base_tx_cost_per_byte: Some(0),
            package_publish_cost_per_byte: Some(80),
            obj_access_cost_read_per_byte: Some(15),
            obj_access_cost_mutate_per_byte: Some(40),
            obj_access_cost_delete_per_byte: Some(40),
            obj_access_cost_verify_per_byte: Some(200),
            obj_data_cost_refundable: Some(100),
            obj_metadata_cost_non_refundable: Some(50),
            gas_model_version: Some(1),
            storage_rebate_rate: Some(9900),
            storage_fund_reinvest_rate: Some(500),
            reward_slashing_rate: Some(5000),
            storage_gas_price: Some(1),
            max_transactions_per_checkpoint: Some(10_000),
            max_checkpoint_size_bytes: Some(30 * 1024 * 1024),

            // For now, perform upgrades with a bare quorum of validators.
            // MUSTFIX: This number should be increased to at least 2000 (20%) for mainnet.
            buffer_stake_for_protocol_upgrade_bps: Some(0),

            // === Native Function Costs ===
            // `address` module
            // Cost params for the Move native function `address::from_bytes(bytes: vector<u8>)`
            address_from_bytes_cost_base: Some(52),
            // Cost params for the Move native function `address::to_u256(address): u256`
            address_to_u256_cost_base: Some(52),
            // Cost params for the Move native function `address::from_u256(u256): address`
            address_from_u256_cost_base: Some(52),

            // `config` module
            // Cost params for the Move native function `read_setting_impl``
            config_read_setting_impl_cost_base: None,
            config_read_setting_impl_cost_per_byte: None,

            // `dynamic_field` module
            // Cost params for the Move native function `hash_type_and_key<K: copy + drop + store>(parent: address, k: K): address`
            dynamic_field_hash_type_and_key_cost_base: Some(100),
            dynamic_field_hash_type_and_key_type_cost_per_byte: Some(2),
            dynamic_field_hash_type_and_key_value_cost_per_byte: Some(2),
            dynamic_field_hash_type_and_key_type_tag_cost_per_byte: Some(2),
            // Cost params for the Move native function `add_child_object<Child: key>(parent: address, child: Child)`
            dynamic_field_add_child_object_cost_base: Some(100),
            dynamic_field_add_child_object_type_cost_per_byte: Some(10),
            dynamic_field_add_child_object_value_cost_per_byte: Some(10),
            dynamic_field_add_child_object_struct_tag_cost_per_byte: Some(10),
            // Cost params for the Move native function `borrow_child_object_mut<Child: key>(parent: &mut UID, id: address): &mut Child`
            dynamic_field_borrow_child_object_cost_base: Some(100),
            dynamic_field_borrow_child_object_child_ref_cost_per_byte: Some(10),
            dynamic_field_borrow_child_object_type_cost_per_byte: Some(10),
            // Cost params for the Move native function `remove_child_object<Child: key>(parent: address, id: address): Child`
            dynamic_field_remove_child_object_cost_base: Some(100),
            dynamic_field_remove_child_object_child_cost_per_byte: Some(2),
            dynamic_field_remove_child_object_type_cost_per_byte: Some(2),
            // Cost params for the Move native function `has_child_object(parent: address, id: address): bool`
            dynamic_field_has_child_object_cost_base: Some(100),
            // Cost params for the Move native function `has_child_object_with_ty<Child: key>(parent: address, id: address): bool`
            dynamic_field_has_child_object_with_ty_cost_base: Some(100),
            dynamic_field_has_child_object_with_ty_type_cost_per_byte: Some(2),
            dynamic_field_has_child_object_with_ty_type_tag_cost_per_byte: Some(2),

            // `event` module
            // Cost params for the Move native function `event::emit<T: copy + drop>(event: T)`
            event_emit_cost_base: Some(52),
            event_emit_value_size_derivation_cost_per_byte: Some(2),
            event_emit_tag_size_derivation_cost_per_byte: Some(5),
            event_emit_output_cost_per_byte: Some(10),

            //  `object` module
            // Cost params for the Move native function `borrow_uid<T: key>(obj: &T): &UID`
            object_borrow_uid_cost_base: Some(52),
            // Cost params for the Move native function `delete_impl(id: address)`
            object_delete_impl_cost_base: Some(52),
            // Cost params for the Move native function `record_new_uid(id: address)`
            object_record_new_uid_cost_base: Some(52),

            // `transfer` module
            // Cost params for the Move native function `transfer_impl<T: key>(obj: T, recipient: address)`
            transfer_transfer_internal_cost_base: Some(52),
            // Cost params for the Move native function `freeze_object<T: key>(obj: T)`
            transfer_freeze_object_cost_base: Some(52),
            // Cost params for the Move native function `share_object<T: key>(obj: T)`
            transfer_share_object_cost_base: Some(52),
            transfer_receive_object_cost_base: None,

            // `tx_context` module
            // Cost params for the Move native function `transfer_impl<T: key>(obj: T, recipient: address)`
            tx_context_derive_id_cost_base: Some(52),

            // `types` module
            // Cost params for the Move native function `is_one_time_witness<T: drop>(_: &T): bool`
            types_is_one_time_witness_cost_base: Some(52),
            types_is_one_time_witness_type_tag_cost_per_byte: Some(2),
            types_is_one_time_witness_type_cost_per_byte: Some(2),

            // `validator` module
            // Cost params for the Move native function `validate_metadata_bcs(metadata: vector<u8>)`
            validator_validate_metadata_cost_base: Some(52),
            validator_validate_metadata_data_cost_per_byte: Some(2),

            // Crypto
            crypto_invalid_arguments_cost: Some(100),
            // bls12381::bls12381_min_pk_verify
            bls12381_bls12381_min_sig_verify_cost_base: Some(52),
            bls12381_bls12381_min_sig_verify_msg_cost_per_byte: Some(2),
            bls12381_bls12381_min_sig_verify_msg_cost_per_block: Some(2),

            // bls12381::bls12381_min_pk_verify
            bls12381_bls12381_min_pk_verify_cost_base: Some(52),
            bls12381_bls12381_min_pk_verify_msg_cost_per_byte: Some(2),
            bls12381_bls12381_min_pk_verify_msg_cost_per_block: Some(2),

            // ecdsa_k1::ecrecover
            ecdsa_k1_ecrecover_keccak256_cost_base: Some(52),
            ecdsa_k1_ecrecover_keccak256_msg_cost_per_byte: Some(2),
            ecdsa_k1_ecrecover_keccak256_msg_cost_per_block: Some(2),
            ecdsa_k1_ecrecover_sha256_cost_base: Some(52),
            ecdsa_k1_ecrecover_sha256_msg_cost_per_byte: Some(2),
            ecdsa_k1_ecrecover_sha256_msg_cost_per_block: Some(2),

            // ecdsa_k1::decompress_pubkey
            ecdsa_k1_decompress_pubkey_cost_base: Some(52),

            // ecdsa_k1::secp256k1_verify
            ecdsa_k1_secp256k1_verify_keccak256_cost_base: Some(52),
            ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_byte: Some(2),
            ecdsa_k1_secp256k1_verify_keccak256_msg_cost_per_block: Some(2),
            ecdsa_k1_secp256k1_verify_sha256_cost_base: Some(52),
            ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_byte: Some(2),
            ecdsa_k1_secp256k1_verify_sha256_msg_cost_per_block: Some(2),

            // ecdsa_r1::ecrecover
            ecdsa_r1_ecrecover_keccak256_cost_base: Some(52),
            ecdsa_r1_ecrecover_keccak256_msg_cost_per_byte: Some(2),
            ecdsa_r1_ecrecover_keccak256_msg_cost_per_block: Some(2),
            ecdsa_r1_ecrecover_sha256_cost_base: Some(52),
            ecdsa_r1_ecrecover_sha256_msg_cost_per_byte: Some(2),
            ecdsa_r1_ecrecover_sha256_msg_cost_per_block: Some(2),

            // ecdsa_r1::secp256k1_verify
            ecdsa_r1_secp256r1_verify_keccak256_cost_base: Some(52),
            ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_byte: Some(2),
            ecdsa_r1_secp256r1_verify_keccak256_msg_cost_per_block: Some(2),
            ecdsa_r1_secp256r1_verify_sha256_cost_base: Some(52),
            ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_byte: Some(2),
            ecdsa_r1_secp256r1_verify_sha256_msg_cost_per_block: Some(2),

            // ecvrf::verify
            ecvrf_ecvrf_verify_cost_base: Some(52),
            ecvrf_ecvrf_verify_alpha_string_cost_per_byte: Some(2),
            ecvrf_ecvrf_verify_alpha_string_cost_per_block: Some(2),

            // ed25519
            ed25519_ed25519_verify_cost_base: Some(52),
            ed25519_ed25519_verify_msg_cost_per_byte: Some(2),
            ed25519_ed25519_verify_msg_cost_per_block: Some(2),

            // groth16::prepare_verifying_key
            groth16_prepare_verifying_key_bls12381_cost_base: Some(52),
            groth16_prepare_verifying_key_bn254_cost_base: Some(52),

            // groth16::verify_groth16_proof_internal
            groth16_verify_groth16_proof_internal_bls12381_cost_base: Some(52),
            groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input: Some(2),
            groth16_verify_groth16_proof_internal_bn254_cost_base: Some(52),
            groth16_verify_groth16_proof_internal_bn254_cost_per_public_input: Some(2),
            groth16_verify_groth16_proof_internal_public_input_cost_per_byte: Some(2),

            // hash::blake2b256
            hash_blake2b256_cost_base: Some(52),
            hash_blake2b256_data_cost_per_byte: Some(2),
            hash_blake2b256_data_cost_per_block: Some(2),
            // hash::keccak256
            hash_keccak256_cost_base: Some(52),
            hash_keccak256_data_cost_per_byte: Some(2),
            hash_keccak256_data_cost_per_block: Some(2),

            poseidon_bn254_cost_base: None,
            poseidon_bn254_cost_per_block: None,

            // hmac::hmac_sha3_256
            hmac_hmac_sha3_256_cost_base: Some(52),
            hmac_hmac_sha3_256_input_cost_per_byte: Some(2),
            hmac_hmac_sha3_256_input_cost_per_block: Some(2),

            // group ops
            group_ops_bls12381_decode_scalar_cost: None,
            group_ops_bls12381_decode_g1_cost: None,
            group_ops_bls12381_decode_g2_cost: None,
            group_ops_bls12381_decode_gt_cost: None,
            group_ops_bls12381_scalar_add_cost: None,
            group_ops_bls12381_g1_add_cost: None,
            group_ops_bls12381_g2_add_cost: None,
            group_ops_bls12381_gt_add_cost: None,
            group_ops_bls12381_scalar_sub_cost: None,
            group_ops_bls12381_g1_sub_cost: None,
            group_ops_bls12381_g2_sub_cost: None,
            group_ops_bls12381_gt_sub_cost: None,
            group_ops_bls12381_scalar_mul_cost: None,
            group_ops_bls12381_g1_mul_cost: None,
            group_ops_bls12381_g2_mul_cost: None,
            group_ops_bls12381_gt_mul_cost: None,
            group_ops_bls12381_scalar_div_cost: None,
            group_ops_bls12381_g1_div_cost: None,
            group_ops_bls12381_g2_div_cost: None,
            group_ops_bls12381_gt_div_cost: None,
            group_ops_bls12381_g1_hash_to_base_cost: None,
            group_ops_bls12381_g2_hash_to_base_cost: None,
            group_ops_bls12381_g1_hash_to_cost_per_byte: None,
            group_ops_bls12381_g2_hash_to_cost_per_byte: None,
            group_ops_bls12381_g1_msm_base_cost: None,
            group_ops_bls12381_g2_msm_base_cost: None,
            group_ops_bls12381_g1_msm_base_cost_per_input: None,
            group_ops_bls12381_g2_msm_base_cost_per_input: None,
            group_ops_bls12381_msm_max_len: None,
            group_ops_bls12381_pairing_cost: None,
            group_ops_bls12381_g1_to_uncompressed_g1_cost: None,
            group_ops_bls12381_uncompressed_g1_to_g1_cost: None,
            group_ops_bls12381_uncompressed_g1_sum_base_cost: None,
            group_ops_bls12381_uncompressed_g1_sum_cost_per_term: None,
            group_ops_bls12381_uncompressed_g1_sum_max_terms: None,

            // zklogin::check_zklogin_id
            check_zklogin_id_cost_base: None,
            // zklogin::check_zklogin_issuer
            check_zklogin_issuer_cost_base: None,

            vdf_verify_vdf_cost: None,
            vdf_hash_to_input_cost: None,

            // nitro_attestation::verify_nitro_attestation
            nitro_attestation_parse_base_cost: None,
            nitro_attestation_parse_cost_per_byte: None,
            nitro_attestation_verify_base_cost: None,
            nitro_attestation_verify_cost_per_cert: None,

            bcs_per_byte_serialized_cost: None,
            bcs_legacy_min_output_size_cost: None,
            bcs_failure_cost: None,
            hash_sha2_256_base_cost: None,
            hash_sha2_256_per_byte_cost: None,
            hash_sha2_256_legacy_min_input_len_cost: None,
            hash_sha3_256_base_cost: None,
            hash_sha3_256_per_byte_cost: None,
            hash_sha3_256_legacy_min_input_len_cost: None,
            type_name_get_base_cost: None,
            type_name_get_per_byte_cost: None,
            string_check_utf8_base_cost: None,
            string_check_utf8_per_byte_cost: None,
            string_is_char_boundary_base_cost: None,
            string_sub_string_base_cost: None,
            string_sub_string_per_byte_cost: None,
            string_index_of_base_cost: None,
            string_index_of_per_byte_pattern_cost: None,
            string_index_of_per_byte_searched_cost: None,
            vector_empty_base_cost: None,
            vector_length_base_cost: None,
            vector_push_back_base_cost: None,
            vector_push_back_legacy_per_abstract_memory_unit_cost: None,
            vector_borrow_base_cost: None,
            vector_pop_back_base_cost: None,
            vector_destroy_empty_base_cost: None,
            vector_swap_base_cost: None,
            debug_print_base_cost: None,
            debug_print_stack_trace_base_cost: None,

            max_size_written_objects: None,
            max_size_written_objects_system_tx: None,

            // ==== Ephemeral (consensus only) params deleted ====
            // Const params for consensus scoring decision
            // scoring_decision_mad_divisor: None,
            // scoring_decision_cutoff_value: None,

            // Limits the length of a Move identifier
            max_move_identifier_len: None,
            max_move_value_depth: None,
            max_move_enum_variants: None,

            gas_rounding_step: None,

            execution_version: None,

            max_event_emit_size_total: None,

            consensus_bad_nodes_stake_threshold: None,

            max_jwk_votes_per_validator_per_epoch: None,

            max_age_of_jwk_in_epochs: None,

            random_beacon_reduction_allowed_delta: None,

            random_beacon_reduction_lower_bound: None,

            random_beacon_dkg_timeout_round: None,

            random_beacon_min_round_interval_ms: None,

            random_beacon_dkg_version: None,

            consensus_max_transaction_size_bytes: None,

            consensus_max_transactions_in_block_bytes: None,

            consensus_max_num_transactions_in_block: None,

            consensus_voting_rounds: None,

            max_accumulated_txn_cost_per_object_in_narwhal_commit: None,

            max_deferral_rounds_for_congestion_control: None,

            max_txn_cost_overage_per_object_in_commit: None,

            allowed_txn_cost_overage_burst_per_object_in_commit: None,

            min_checkpoint_interval_ms: None,

            checkpoint_summary_version_specific_data: None,

            max_soft_bundle_size: None,

            bridge_should_try_to_finalize_committee: None,

            max_accumulated_txn_cost_per_object_in_mysticeti_commit: None,

            max_accumulated_randomness_txn_cost_per_object_in_mysticeti_commit: None,

            consensus_gc_depth: None,

            gas_budget_based_txn_cost_cap_factor: None,

            gas_budget_based_txn_cost_absolute_cap_commit_count: None,

            sip_45_consensus_amplification_threshold: None,

            use_object_per_epoch_marker_table_v2: None,
            // When adding a new constant, set it to None in the earliest version, like this:
            // new_constant: None,
        };
        for cur in 2..=version.0 {
            match cur {
                1 => unreachable!(),
                2 => {
                    cfg.feature_flags.advance_epoch_start_time_in_safe_mode = true;
                }
                3 => {
                    // changes for gas model
                    cfg.gas_model_version = Some(2);
                    // max gas budget is in MIST and an absolute value 50SUI
                    cfg.max_tx_gas = Some(50_000_000_000);
                    // min gas budget is in MIST and an absolute value 2000MIST or 0.000002SUI
                    cfg.base_tx_cost_fixed = Some(2_000);
                    // storage gas price multiplier
                    cfg.storage_gas_price = Some(76);
                    cfg.feature_flags.loaded_child_objects_fixed = true;
                    // max size of written objects during a TXn
                    // this is a sum of all objects written during a TXn
                    cfg.max_size_written_objects = Some(5 * 1000 * 1000);
                    // max size of written objects during a system TXn to allow for larger writes
                    // akin to `max_size_written_objects` but for system TXns
                    cfg.max_size_written_objects_system_tx = Some(50 * 1000 * 1000);
                    cfg.feature_flags.package_upgrades = true;
                }
                // This is the first protocol version currently possible.
                // Mainnet starts with version 4. Previous versions are pre mainnet and have
                // all been wiped out.
                // Every other chain is after version 4.
                4 => {
                    // Change reward slashing rate to 100%.
                    cfg.reward_slashing_rate = Some(10000);
                    // protect old and new lookup for object version
                    cfg.gas_model_version = Some(3);
                }
                5 => {
                    cfg.feature_flags.missing_type_is_compatibility_error = true;
                    cfg.gas_model_version = Some(4);
                    cfg.feature_flags.scoring_decision_with_validity_cutoff = true;
                    // ==== Ephemeral (consensus only) params deleted ====
                    // cfg.scoring_decision_mad_divisor = Some(2.3);
                    // cfg.scoring_decision_cutoff_value = Some(2.5);
                }
                6 => {
                    cfg.gas_model_version = Some(5);
                    cfg.buffer_stake_for_protocol_upgrade_bps = Some(5000);
                    cfg.feature_flags.consensus_order_end_of_epoch_last = true;
                }
                7 => {
                    cfg.feature_flags.disallow_adding_abilities_on_upgrade = true;
                    cfg.feature_flags
                        .disable_invariant_violation_check_in_swap_loc = true;
                    cfg.feature_flags.ban_entry_init = true;
                    cfg.feature_flags.package_digest_hash_module = true;
                }
                8 => {
                    cfg.feature_flags
                        .disallow_change_struct_type_params_on_upgrade = true;
                }
                9 => {
                    // Limits the length of a Move identifier
                    cfg.max_move_identifier_len = Some(128);
                    cfg.feature_flags.no_extraneous_module_bytes = true;
                    cfg.feature_flags
                        .advance_to_highest_supported_protocol_version = true;
                }
                10 => {
                    cfg.max_verifier_meter_ticks_per_function = Some(16_000_000);
                    cfg.max_meter_ticks_per_module = Some(16_000_000);
                }
                11 => {
                    cfg.max_move_value_depth = Some(128);
                }
                12 => {
                    cfg.feature_flags.narwhal_versioned_metadata = true;
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.commit_root_state_digest = true;
                    }

                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.zklogin_auth = true;
                    }
                }
                13 => {}
                14 => {
                    cfg.gas_rounding_step = Some(1_000);
                    cfg.gas_model_version = Some(6);
                }
                15 => {
                    cfg.feature_flags.consensus_transaction_ordering =
                        ConsensusTransactionOrdering::ByGasPrice;
                }
                16 => {
                    cfg.feature_flags.simplified_unwrap_then_delete = true;
                }
                17 => {
                    cfg.feature_flags.upgraded_multisig_supported = true;
                }
                18 => {
                    cfg.execution_version = Some(1);
                    // Following flags are implied by this execution version.  Once support for earlier
                    // protocol versions is dropped, these flags can be removed:
                    // cfg.feature_flags.package_upgrades = true;
                    // cfg.feature_flags.disallow_adding_abilities_on_upgrade = true;
                    // cfg.feature_flags.disallow_change_struct_type_params_on_upgrade = true;
                    // cfg.feature_flags.loaded_child_objects_fixed = true;
                    // cfg.feature_flags.ban_entry_init = true;
                    // cfg.feature_flags.pack_digest_hash_modules = true;
                    cfg.feature_flags.txn_base_cost_as_multiplier = true;
                    // this is a multiplier of the gas price
                    cfg.base_tx_cost_fixed = Some(1_000);
                }
                19 => {
                    cfg.max_num_event_emit = Some(1024);
                    // We maintain the same total size limit for events, but increase the number of
                    // events that can be emitted.
                    cfg.max_event_emit_size_total = Some(
                        256 /* former event count limit */ * 250 * 1024, /* size limit per event */
                    );
                }
                20 => {
                    cfg.feature_flags.commit_root_state_digest = true;

                    if chain != Chain::Mainnet {
                        cfg.feature_flags.narwhal_new_leader_election_schedule = true;
                        cfg.consensus_bad_nodes_stake_threshold = Some(20);
                    }
                }

                21 => {
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.zklogin_supported_providers = BTreeSet::from([
                            "Google".to_string(),
                            "Facebook".to_string(),
                            "Twitch".to_string(),
                        ]);
                    }
                }
                22 => {
                    cfg.feature_flags.loaded_child_object_format = true;
                }
                23 => {
                    cfg.feature_flags.loaded_child_object_format_type = true;
                    cfg.feature_flags.narwhal_new_leader_election_schedule = true;
                    // Taking a baby step approach, we consider only 20% by stake as bad nodes so we
                    // have a 80% by stake of nodes participating in the leader committee. That allow
                    // us for more redundancy in case we have validators under performing - since the
                    // responsibility is shared amongst more nodes. We can increase that once we do have
                    // higher confidence.
                    cfg.consensus_bad_nodes_stake_threshold = Some(20);
                }
                24 => {
                    cfg.feature_flags.simple_conservation_checks = true;
                    cfg.max_publish_or_upgrade_per_ptb = Some(5);

                    cfg.feature_flags.end_of_epoch_transaction_supported = true;

                    if chain != Chain::Mainnet {
                        cfg.feature_flags.enable_jwk_consensus_updates = true;
                        // Max of 10 votes per hour
                        cfg.max_jwk_votes_per_validator_per_epoch = Some(240);
                        cfg.max_age_of_jwk_in_epochs = Some(1);
                    }
                }
                25 => {
                    // Enable zkLogin for all providers in all networks.
                    cfg.feature_flags.zklogin_supported_providers = BTreeSet::from([
                        "Google".to_string(),
                        "Facebook".to_string(),
                        "Twitch".to_string(),
                    ]);
                    cfg.feature_flags.zklogin_auth = true;

                    // Enable jwk consensus updates
                    cfg.feature_flags.enable_jwk_consensus_updates = true;
                    cfg.max_jwk_votes_per_validator_per_epoch = Some(240);
                    cfg.max_age_of_jwk_in_epochs = Some(1);
                }
                26 => {
                    cfg.gas_model_version = Some(7);
                    // Only enable receiving objects in devnet
                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.transfer_receive_object_cost_base = Some(52);
                        cfg.feature_flags.receive_objects = true;
                    }
                }
                27 => {
                    cfg.gas_model_version = Some(8);
                }
                28 => {
                    // zklogin::check_zklogin_id
                    cfg.check_zklogin_id_cost_base = Some(200);
                    // zklogin::check_zklogin_issuer
                    cfg.check_zklogin_issuer_cost_base = Some(200);

                    // Only enable effects v2 on devnet.
                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.enable_effects_v2 = true;
                    }
                }
                29 => {
                    cfg.feature_flags.verify_legacy_zklogin_address = true;
                }
                30 => {
                    // Only enable nw certificate v2 on testnet.
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.narwhal_certificate_v2 = true;
                    }

                    cfg.random_beacon_reduction_allowed_delta = Some(800);
                    // Only enable effects v2 on devnet and testnet.
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.enable_effects_v2 = true;
                    }

                    // zklogin_supported_providers config is deprecated, zklogin
                    // signature verifier will use the fetched jwk map to determine
                    // whether the provider is supported based on node config.
                    cfg.feature_flags.zklogin_supported_providers = BTreeSet::default();

                    cfg.feature_flags.recompute_has_public_transfer_in_execution = true;
                }
                31 => {
                    cfg.execution_version = Some(2);
                    // Only enable shared object deletion on devnet
                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.shared_object_deletion = true;
                    }
                }
                32 => {
                    // enable zklogin in multisig in devnet and testnet
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.accept_zklogin_in_multisig = true;
                    }
                    // enable receiving objects in devnet and testnet
                    if chain != Chain::Mainnet {
                        cfg.transfer_receive_object_cost_base = Some(52);
                        cfg.feature_flags.receive_objects = true;
                    }
                    // Only enable random beacon on devnet
                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.random_beacon = true;
                        cfg.random_beacon_reduction_lower_bound = Some(1600);
                        cfg.random_beacon_dkg_timeout_round = Some(3000);
                        cfg.random_beacon_min_round_interval_ms = Some(150);
                    }
                    // Only enable consensus digest in consensus commit prologue in devnet.
                    if chain != Chain::Testnet && chain != Chain::Mainnet {
                        cfg.feature_flags.include_consensus_digest_in_prologue = true;
                    }

                    // enable nw cert v2 on mainnet
                    cfg.feature_flags.narwhal_certificate_v2 = true;
                }
                33 => {
                    cfg.feature_flags.hardened_otw_check = true;
                    cfg.feature_flags.allow_receiving_object_id = true;

                    // Enable transfer-to-object in mainnet
                    cfg.transfer_receive_object_cost_base = Some(52);
                    cfg.feature_flags.receive_objects = true;

                    // Enable shared object deletion in testnet and devnet
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.shared_object_deletion = true;
                    }

                    cfg.feature_flags.enable_effects_v2 = true;
                }
                34 => {}
                35 => {
                    // Add costs for poseidon::poseidon_bn254
                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.enable_poseidon = true;
                        cfg.poseidon_bn254_cost_base = Some(260);
                        cfg.poseidon_bn254_cost_per_block = Some(10);
                    }

                    cfg.feature_flags.enable_coin_deny_list = true;
                }
                36 => {
                    // Only enable group ops on devnet
                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.enable_group_ops_native_functions = true;
                        cfg.feature_flags.enable_group_ops_native_function_msm = true;
                        // Next values are arbitrary in a similar way as the other crypto native functions.
                        cfg.group_ops_bls12381_decode_scalar_cost = Some(52);
                        cfg.group_ops_bls12381_decode_g1_cost = Some(52);
                        cfg.group_ops_bls12381_decode_g2_cost = Some(52);
                        cfg.group_ops_bls12381_decode_gt_cost = Some(52);
                        cfg.group_ops_bls12381_scalar_add_cost = Some(52);
                        cfg.group_ops_bls12381_g1_add_cost = Some(52);
                        cfg.group_ops_bls12381_g2_add_cost = Some(52);
                        cfg.group_ops_bls12381_gt_add_cost = Some(52);
                        cfg.group_ops_bls12381_scalar_sub_cost = Some(52);
                        cfg.group_ops_bls12381_g1_sub_cost = Some(52);
                        cfg.group_ops_bls12381_g2_sub_cost = Some(52);
                        cfg.group_ops_bls12381_gt_sub_cost = Some(52);
                        cfg.group_ops_bls12381_scalar_mul_cost = Some(52);
                        cfg.group_ops_bls12381_g1_mul_cost = Some(52);
                        cfg.group_ops_bls12381_g2_mul_cost = Some(52);
                        cfg.group_ops_bls12381_gt_mul_cost = Some(52);
                        cfg.group_ops_bls12381_scalar_div_cost = Some(52);
                        cfg.group_ops_bls12381_g1_div_cost = Some(52);
                        cfg.group_ops_bls12381_g2_div_cost = Some(52);
                        cfg.group_ops_bls12381_gt_div_cost = Some(52);
                        cfg.group_ops_bls12381_g1_hash_to_base_cost = Some(52);
                        cfg.group_ops_bls12381_g2_hash_to_base_cost = Some(52);
                        cfg.group_ops_bls12381_g1_hash_to_cost_per_byte = Some(2);
                        cfg.group_ops_bls12381_g2_hash_to_cost_per_byte = Some(2);
                        cfg.group_ops_bls12381_g1_msm_base_cost = Some(52);
                        cfg.group_ops_bls12381_g2_msm_base_cost = Some(52);
                        cfg.group_ops_bls12381_g1_msm_base_cost_per_input = Some(52);
                        cfg.group_ops_bls12381_g2_msm_base_cost_per_input = Some(52);
                        cfg.group_ops_bls12381_msm_max_len = Some(32);
                        cfg.group_ops_bls12381_pairing_cost = Some(52);
                    }
                    // Enable shared object deletion on all networks.
                    cfg.feature_flags.shared_object_deletion = true;

                    cfg.consensus_max_transaction_size_bytes = Some(256 * 1024); // 256KB
                    cfg.consensus_max_transactions_in_block_bytes = Some(6 * 1_024 * 1024);
                    // 6 MB
                }
                37 => {
                    cfg.feature_flags.reject_mutable_random_on_entry_functions = true;

                    // Enable consensus digest in consensus commit prologue in testnet and devnet.
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.include_consensus_digest_in_prologue = true;
                    }
                }
                38 => {
                    cfg.binary_module_handles = Some(100);
                    cfg.binary_struct_handles = Some(300);
                    cfg.binary_function_handles = Some(1500);
                    cfg.binary_function_instantiations = Some(750);
                    cfg.binary_signatures = Some(1000);
                    // constants and identifiers are proportional to the binary size,
                    // and they vastly depend on the code, so we are leaving them
                    // reasonably high
                    cfg.binary_constant_pool = Some(4000);
                    cfg.binary_identifiers = Some(10000);
                    cfg.binary_address_identifiers = Some(100);
                    cfg.binary_struct_defs = Some(200);
                    cfg.binary_struct_def_instantiations = Some(100);
                    cfg.binary_function_defs = Some(1000);
                    cfg.binary_field_handles = Some(500);
                    cfg.binary_field_instantiations = Some(250);
                    cfg.binary_friend_decls = Some(100);
                    // reduce dependencies maximum
                    cfg.max_package_dependencies = Some(32);
                    cfg.max_modules_in_publish = Some(64);
                    // bump execution version
                    cfg.execution_version = Some(3);
                }
                39 => {
                    // It is important that we keep this protocol version blank due to an issue with random.move.
                }
                40 => {}
                41 => {
                    // Enable group ops and all networks (but not msm)
                    cfg.feature_flags.enable_group_ops_native_functions = true;
                    // Next values are arbitrary in a similar way as the other crypto native functions.
                    cfg.group_ops_bls12381_decode_scalar_cost = Some(52);
                    cfg.group_ops_bls12381_decode_g1_cost = Some(52);
                    cfg.group_ops_bls12381_decode_g2_cost = Some(52);
                    cfg.group_ops_bls12381_decode_gt_cost = Some(52);
                    cfg.group_ops_bls12381_scalar_add_cost = Some(52);
                    cfg.group_ops_bls12381_g1_add_cost = Some(52);
                    cfg.group_ops_bls12381_g2_add_cost = Some(52);
                    cfg.group_ops_bls12381_gt_add_cost = Some(52);
                    cfg.group_ops_bls12381_scalar_sub_cost = Some(52);
                    cfg.group_ops_bls12381_g1_sub_cost = Some(52);
                    cfg.group_ops_bls12381_g2_sub_cost = Some(52);
                    cfg.group_ops_bls12381_gt_sub_cost = Some(52);
                    cfg.group_ops_bls12381_scalar_mul_cost = Some(52);
                    cfg.group_ops_bls12381_g1_mul_cost = Some(52);
                    cfg.group_ops_bls12381_g2_mul_cost = Some(52);
                    cfg.group_ops_bls12381_gt_mul_cost = Some(52);
                    cfg.group_ops_bls12381_scalar_div_cost = Some(52);
                    cfg.group_ops_bls12381_g1_div_cost = Some(52);
                    cfg.group_ops_bls12381_g2_div_cost = Some(52);
                    cfg.group_ops_bls12381_gt_div_cost = Some(52);
                    cfg.group_ops_bls12381_g1_hash_to_base_cost = Some(52);
                    cfg.group_ops_bls12381_g2_hash_to_base_cost = Some(52);
                    cfg.group_ops_bls12381_g1_hash_to_cost_per_byte = Some(2);
                    cfg.group_ops_bls12381_g2_hash_to_cost_per_byte = Some(2);
                    cfg.group_ops_bls12381_g1_msm_base_cost = Some(52);
                    cfg.group_ops_bls12381_g2_msm_base_cost = Some(52);
                    cfg.group_ops_bls12381_g1_msm_base_cost_per_input = Some(52);
                    cfg.group_ops_bls12381_g2_msm_base_cost_per_input = Some(52);
                    cfg.group_ops_bls12381_msm_max_len = Some(32);
                    cfg.group_ops_bls12381_pairing_cost = Some(52);
                }
                42 => {}
                43 => {
                    cfg.feature_flags.zklogin_max_epoch_upper_bound_delta = Some(30);
                    cfg.max_meter_ticks_per_package = Some(16_000_000);
                }
                44 => {
                    // Enable consensus digest in consensus commit prologue on all networks..
                    cfg.feature_flags.include_consensus_digest_in_prologue = true;
                    // Switch between Narwhal and Mysticeti per epoch in tests, devnet and testnet.
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.consensus_choice = ConsensusChoice::SwapEachEpoch;
                    }
                }
                45 => {
                    // Use tonic networking for consensus, in tests and devnet.
                    if chain != Chain::Testnet && chain != Chain::Mainnet {
                        cfg.feature_flags.consensus_network = ConsensusNetwork::Tonic;
                    }

                    if chain != Chain::Mainnet {
                        // Enable leader scoring & schedule change on testnet for mysticeti.
                        cfg.feature_flags.mysticeti_leader_scoring_and_schedule = true;
                    }
                    cfg.min_move_binary_format_version = Some(6);
                    cfg.feature_flags.accept_zklogin_in_multisig = true;

                    // Also bumps framework snapshot to fix binop issue.

                    // enable bridge in devnet
                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.bridge = true;
                    }
                }
                46 => {
                    // enable bridge in devnet and testnet
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.bridge = true;
                    }

                    // Enable resharing at same initial version
                    cfg.feature_flags.reshare_at_same_initial_version = true;
                }
                47 => {}
                48 => {
                    // Use tonic networking for Mysticeti.
                    cfg.feature_flags.consensus_network = ConsensusNetwork::Tonic;

                    // Enable resolving abort code IDs to package ID instead of runtime module ID
                    cfg.feature_flags.resolve_abort_locations_to_package_id = true;

                    // Enable random beacon on testnet.
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.random_beacon = true;
                        cfg.random_beacon_reduction_lower_bound = Some(1600);
                        cfg.random_beacon_dkg_timeout_round = Some(3000);
                        cfg.random_beacon_min_round_interval_ms = Some(200);
                    }

                    // Enable the committed sub dag digest inclusion on the commit output
                    cfg.feature_flags.mysticeti_use_committed_subdag_digest = true;
                }
                49 => {
                    if chain != Chain::Testnet && chain != Chain::Mainnet {
                        cfg.move_binary_format_version = Some(7);
                    }

                    // enable vdf in devnet
                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.enable_vdf = true;
                        // Set to 30x and 2x the cost of a signature verification for now. This
                        // should be updated along with other native crypto functions.
                        cfg.vdf_verify_vdf_cost = Some(1500);
                        cfg.vdf_hash_to_input_cost = Some(100);
                    }

                    // Only enable consensus commit prologue V3 in devnet.
                    if chain != Chain::Testnet && chain != Chain::Mainnet {
                        cfg.feature_flags
                            .record_consensus_determined_version_assignments_in_prologue = true;
                    }

                    // Run Mysticeti consensus in testnet.
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.consensus_choice = ConsensusChoice::Mysticeti;
                    }

                    // Run Move verification on framework upgrades in its own VM
                    cfg.feature_flags.fresh_vm_on_framework_upgrade = true;
                }
                50 => {
                    // Enable checkpoint batching in testnet.
                    if chain != Chain::Mainnet {
                        cfg.checkpoint_summary_version_specific_data = Some(1);
                        cfg.min_checkpoint_interval_ms = Some(200);
                    }

                    // Only enable prepose consensus commit prologue in checkpoints in devnet.
                    if chain != Chain::Testnet && chain != Chain::Mainnet {
                        cfg.feature_flags
                            .prepend_prologue_tx_in_consensus_commit_in_checkpoints = true;
                    }

                    cfg.feature_flags.mysticeti_num_leaders_per_round = Some(1);

                    // Set max transaction deferral to 10 consensus rounds.
                    cfg.max_deferral_rounds_for_congestion_control = Some(10);
                }
                51 => {
                    cfg.random_beacon_dkg_version = Some(1);

                    if chain != Chain::Testnet && chain != Chain::Mainnet {
                        cfg.feature_flags.enable_coin_deny_list_v2 = true;
                    }
                }
                52 => {
                    if chain != Chain::Mainnet {
                        cfg.feature_flags.soft_bundle = true;
                        cfg.max_soft_bundle_size = Some(5);
                    }

                    cfg.config_read_setting_impl_cost_base = Some(100);
                    cfg.config_read_setting_impl_cost_per_byte = Some(40);

                    // Turn on shared object congestion control in devnet.
                    if chain != Chain::Testnet && chain != Chain::Mainnet {
                        cfg.max_accumulated_txn_cost_per_object_in_narwhal_commit = Some(100);
                        cfg.feature_flags.per_object_congestion_control_mode =
                            PerObjectCongestionControlMode::TotalTxCount;
                    }

                    // Enable Mysticeti on mainnet.
                    cfg.feature_flags.consensus_choice = ConsensusChoice::Mysticeti;

                    // Enable leader scoring & schedule change on mainnet for mysticeti.
                    cfg.feature_flags.mysticeti_leader_scoring_and_schedule = true;

                    // Enable checkpoint batching on mainnet.
                    cfg.checkpoint_summary_version_specific_data = Some(1);
                    cfg.min_checkpoint_interval_ms = Some(200);

                    // Enable consensus commit prologue V3 in testnet.
                    if chain != Chain::Mainnet {
                        cfg.feature_flags
                            .record_consensus_determined_version_assignments_in_prologue = true;
                        cfg.feature_flags
                            .prepend_prologue_tx_in_consensus_commit_in_checkpoints = true;
                    }
                    // Turn on enums in testnet and devnet
                    if chain != Chain::Mainnet {
                        cfg.move_binary_format_version = Some(7);
                    }

                    if chain != Chain::Testnet && chain != Chain::Mainnet {
                        cfg.feature_flags.passkey_auth = true;
                    }
                    cfg.feature_flags.enable_coin_deny_list_v2 = true;
                }
                53 => {
                    // Do not allow bridge committee to finalize on mainnet.
                    cfg.bridge_should_try_to_finalize_committee = Some(chain != Chain::Mainnet);

                    // Enable consensus commit prologue V3 on mainnet.
                    cfg.feature_flags
                        .record_consensus_determined_version_assignments_in_prologue = true;
                    cfg.feature_flags
                        .prepend_prologue_tx_in_consensus_commit_in_checkpoints = true;

                    if chain == Chain::Unknown {
                        cfg.feature_flags.authority_capabilities_v2 = true;
                    }

                    // Turns on shared object congestion control on testnet.
                    if chain != Chain::Mainnet {
                        cfg.max_accumulated_txn_cost_per_object_in_narwhal_commit = Some(100);
                        cfg.max_accumulated_txn_cost_per_object_in_mysticeti_commit = Some(10);
                        cfg.feature_flags.per_object_congestion_control_mode =
                            PerObjectCongestionControlMode::TotalTxCount;
                    }

                    // Adjust stdlib gas costs
                    cfg.bcs_per_byte_serialized_cost = Some(2);
                    cfg.bcs_legacy_min_output_size_cost = Some(1);
                    cfg.bcs_failure_cost = Some(52);
                    cfg.debug_print_base_cost = Some(52);
                    cfg.debug_print_stack_trace_base_cost = Some(52);
                    cfg.hash_sha2_256_base_cost = Some(52);
                    cfg.hash_sha2_256_per_byte_cost = Some(2);
                    cfg.hash_sha2_256_legacy_min_input_len_cost = Some(1);
                    cfg.hash_sha3_256_base_cost = Some(52);
                    cfg.hash_sha3_256_per_byte_cost = Some(2);
                    cfg.hash_sha3_256_legacy_min_input_len_cost = Some(1);
                    cfg.type_name_get_base_cost = Some(52);
                    cfg.type_name_get_per_byte_cost = Some(2);
                    cfg.string_check_utf8_base_cost = Some(52);
                    cfg.string_check_utf8_per_byte_cost = Some(2);
                    cfg.string_is_char_boundary_base_cost = Some(52);
                    cfg.string_sub_string_base_cost = Some(52);
                    cfg.string_sub_string_per_byte_cost = Some(2);
                    cfg.string_index_of_base_cost = Some(52);
                    cfg.string_index_of_per_byte_pattern_cost = Some(2);
                    cfg.string_index_of_per_byte_searched_cost = Some(2);
                    cfg.vector_empty_base_cost = Some(52);
                    cfg.vector_length_base_cost = Some(52);
                    cfg.vector_push_back_base_cost = Some(52);
                    cfg.vector_push_back_legacy_per_abstract_memory_unit_cost = Some(2);
                    cfg.vector_borrow_base_cost = Some(52);
                    cfg.vector_pop_back_base_cost = Some(52);
                    cfg.vector_destroy_empty_base_cost = Some(52);
                    cfg.vector_swap_base_cost = Some(52);
                }
                54 => {
                    // Enable random beacon on mainnet.
                    cfg.feature_flags.random_beacon = true;
                    cfg.random_beacon_reduction_lower_bound = Some(1000);
                    cfg.random_beacon_dkg_timeout_round = Some(3000);
                    cfg.random_beacon_min_round_interval_ms = Some(500);

                    // Turns on shared object congestion control on mainnet.
                    cfg.max_accumulated_txn_cost_per_object_in_narwhal_commit = Some(100);
                    cfg.max_accumulated_txn_cost_per_object_in_mysticeti_commit = Some(10);
                    cfg.feature_flags.per_object_congestion_control_mode =
                        PerObjectCongestionControlMode::TotalTxCount;

                    // Enable soft bundle on mainnet.
                    cfg.feature_flags.soft_bundle = true;
                    cfg.max_soft_bundle_size = Some(5);
                }
                55 => {
                    // Turn on enums mainnet
                    cfg.move_binary_format_version = Some(7);

                    // Assume 1KB per transaction and 500 transactions per block.
                    cfg.consensus_max_transactions_in_block_bytes = Some(512 * 1024);
                    // Assume 20_000 TPS * 5% max stake per validator / (minimum) 4 blocks per round = 250 transactions per block maximum
                    // Using a higher limit that is 512, to account for bursty traffic and system transactions.
                    cfg.consensus_max_num_transactions_in_block = Some(512);

                    cfg.feature_flags.rethrow_serialization_type_layout_errors = true;
                }
                56 => {
                    if chain == Chain::Mainnet {
                        cfg.feature_flags.bridge = true;
                    }
                }
                57 => {
                    // Reduce minimum number of random beacon shares.
                    cfg.random_beacon_reduction_lower_bound = Some(800);
                }
                58 => {
                    if chain == Chain::Mainnet {
                        cfg.bridge_should_try_to_finalize_committee = Some(true);
                    }

                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        // Enable distributed vote scoring for devnet
                        cfg.feature_flags
                            .consensus_distributed_vote_scoring_strategy = true;
                    }
                }
                59 => {
                    // Enable round prober in consensus.
                    cfg.feature_flags.consensus_round_prober = true;
                }
                60 => {
                    cfg.max_type_to_layout_nodes = Some(512);
                    cfg.feature_flags.validate_identifier_inputs = true;
                }
                61 => {
                    if chain != Chain::Mainnet {
                        // Enable distributed vote scoring for testnet
                        cfg.feature_flags
                            .consensus_distributed_vote_scoring_strategy = true;
                    }
                    // Further reduce minimum number of random beacon shares.
                    cfg.random_beacon_reduction_lower_bound = Some(700);

                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        // Enable Mysticeti fastpath for devnet
                        cfg.feature_flags.mysticeti_fastpath = true;
                    }
                }
                62 => {
                    cfg.feature_flags.relocate_event_module = true;
                }
                63 => {
                    cfg.feature_flags.per_object_congestion_control_mode =
                        PerObjectCongestionControlMode::TotalGasBudgetWithCap;
                    cfg.gas_budget_based_txn_cost_cap_factor = Some(400_000);
                    cfg.max_accumulated_txn_cost_per_object_in_mysticeti_commit = Some(18_500_000);
                    cfg.max_accumulated_txn_cost_per_object_in_narwhal_commit = Some(240_000_000);
                }
                64 => {
                    cfg.feature_flags.per_object_congestion_control_mode =
                        PerObjectCongestionControlMode::TotalTxCount;
                    cfg.max_accumulated_txn_cost_per_object_in_narwhal_commit = Some(40);
                    cfg.max_accumulated_txn_cost_per_object_in_mysticeti_commit = Some(3);
                }
                65 => {
                    // Enable distributed vote scoring for mainnet
                    cfg.feature_flags
                        .consensus_distributed_vote_scoring_strategy = true;
                }
                66 => {
                    if chain == Chain::Mainnet {
                        // Revert the distributed vote scoring for mainnet (for one protocol upgrade)
                        cfg.feature_flags
                            .consensus_distributed_vote_scoring_strategy = false;
                    }
                }
                67 => {
                    // Enable it once again.
                    cfg.feature_flags
                        .consensus_distributed_vote_scoring_strategy = true;
                }
                68 => {
                    cfg.group_ops_bls12381_g1_to_uncompressed_g1_cost = Some(26);
                    cfg.group_ops_bls12381_uncompressed_g1_to_g1_cost = Some(52);
                    cfg.group_ops_bls12381_uncompressed_g1_sum_base_cost = Some(26);
                    cfg.group_ops_bls12381_uncompressed_g1_sum_cost_per_term = Some(13);
                    cfg.group_ops_bls12381_uncompressed_g1_sum_max_terms = Some(2000);

                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.uncompressed_g1_group_elements = true;
                    }

                    cfg.feature_flags.per_object_congestion_control_mode =
                        PerObjectCongestionControlMode::TotalGasBudgetWithCap;
                    cfg.gas_budget_based_txn_cost_cap_factor = Some(400_000);
                    cfg.max_accumulated_txn_cost_per_object_in_mysticeti_commit = Some(18_500_000);
                    cfg.max_accumulated_randomness_txn_cost_per_object_in_mysticeti_commit =
                        Some(3_700_000); // 20% of above
                    cfg.max_txn_cost_overage_per_object_in_commit = Some(u64::MAX);
                    cfg.gas_budget_based_txn_cost_absolute_cap_commit_count = Some(50);

                    // Further reduce minimum number of random beacon shares.
                    cfg.random_beacon_reduction_lower_bound = Some(500);

                    cfg.feature_flags.disallow_new_modules_in_deps_only_packages = true;
                }
                69 => {
                    // Sets number of rounds allowed for fastpath voting in consensus.
                    cfg.consensus_voting_rounds = Some(40);

                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        // Enable smart ancestor selection for devnet
                        cfg.feature_flags.consensus_smart_ancestor_selection = true;
                    }

                    if chain != Chain::Mainnet {
                        cfg.feature_flags.uncompressed_g1_group_elements = true;
                    }
                }
                70 => {
                    if chain != Chain::Mainnet {
                        // Enable smart ancestor selection for testnet
                        cfg.feature_flags.consensus_smart_ancestor_selection = true;
                        // Enable probing for accepted rounds in round prober for testnet
                        cfg.feature_flags
                            .consensus_round_prober_probe_accepted_rounds = true;
                    }

                    cfg.poseidon_bn254_cost_per_block = Some(388);

                    cfg.gas_model_version = Some(9);
                    cfg.feature_flags.native_charging_v2 = true;
                    cfg.bls12381_bls12381_min_sig_verify_cost_base = Some(44064);
                    cfg.bls12381_bls12381_min_pk_verify_cost_base = Some(49282);
                    cfg.ecdsa_k1_secp256k1_verify_keccak256_cost_base = Some(1470);
                    cfg.ecdsa_k1_secp256k1_verify_sha256_cost_base = Some(1470);
                    cfg.ecdsa_r1_secp256r1_verify_sha256_cost_base = Some(4225);
                    cfg.ecdsa_r1_secp256r1_verify_keccak256_cost_base = Some(4225);
                    cfg.ecvrf_ecvrf_verify_cost_base = Some(4848);
                    cfg.ed25519_ed25519_verify_cost_base = Some(1802);

                    // Manually changed to be "under cost"
                    cfg.ecdsa_r1_ecrecover_keccak256_cost_base = Some(1173);
                    cfg.ecdsa_r1_ecrecover_sha256_cost_base = Some(1173);
                    cfg.ecdsa_k1_ecrecover_keccak256_cost_base = Some(500);
                    cfg.ecdsa_k1_ecrecover_sha256_cost_base = Some(500);

                    cfg.groth16_prepare_verifying_key_bls12381_cost_base = Some(53838);
                    cfg.groth16_prepare_verifying_key_bn254_cost_base = Some(82010);
                    cfg.groth16_verify_groth16_proof_internal_bls12381_cost_base = Some(72090);
                    cfg.groth16_verify_groth16_proof_internal_bls12381_cost_per_public_input =
                        Some(8213);
                    cfg.groth16_verify_groth16_proof_internal_bn254_cost_base = Some(115502);
                    cfg.groth16_verify_groth16_proof_internal_bn254_cost_per_public_input =
                        Some(9484);

                    cfg.hash_keccak256_cost_base = Some(10);
                    cfg.hash_blake2b256_cost_base = Some(10);

                    // group ops
                    cfg.group_ops_bls12381_decode_scalar_cost = Some(7);
                    cfg.group_ops_bls12381_decode_g1_cost = Some(2848);
                    cfg.group_ops_bls12381_decode_g2_cost = Some(3770);
                    cfg.group_ops_bls12381_decode_gt_cost = Some(3068);

                    cfg.group_ops_bls12381_scalar_add_cost = Some(10);
                    cfg.group_ops_bls12381_g1_add_cost = Some(1556);
                    cfg.group_ops_bls12381_g2_add_cost = Some(3048);
                    cfg.group_ops_bls12381_gt_add_cost = Some(188);

                    cfg.group_ops_bls12381_scalar_sub_cost = Some(10);
                    cfg.group_ops_bls12381_g1_sub_cost = Some(1550);
                    cfg.group_ops_bls12381_g2_sub_cost = Some(3019);
                    cfg.group_ops_bls12381_gt_sub_cost = Some(497);

                    cfg.group_ops_bls12381_scalar_mul_cost = Some(11);
                    cfg.group_ops_bls12381_g1_mul_cost = Some(4842);
                    cfg.group_ops_bls12381_g2_mul_cost = Some(9108);
                    cfg.group_ops_bls12381_gt_mul_cost = Some(27490);

                    cfg.group_ops_bls12381_scalar_div_cost = Some(91);
                    cfg.group_ops_bls12381_g1_div_cost = Some(5091);
                    cfg.group_ops_bls12381_g2_div_cost = Some(9206);
                    cfg.group_ops_bls12381_gt_div_cost = Some(27804);

                    cfg.group_ops_bls12381_g1_hash_to_base_cost = Some(2962);
                    cfg.group_ops_bls12381_g2_hash_to_base_cost = Some(8688);

                    cfg.group_ops_bls12381_g1_msm_base_cost = Some(62648);
                    cfg.group_ops_bls12381_g2_msm_base_cost = Some(131192);
                    cfg.group_ops_bls12381_g1_msm_base_cost_per_input = Some(1333);
                    cfg.group_ops_bls12381_g2_msm_base_cost_per_input = Some(3216);

                    cfg.group_ops_bls12381_uncompressed_g1_to_g1_cost = Some(677);
                    cfg.group_ops_bls12381_g1_to_uncompressed_g1_cost = Some(2099);
                    cfg.group_ops_bls12381_uncompressed_g1_sum_base_cost = Some(77);
                    cfg.group_ops_bls12381_uncompressed_g1_sum_cost_per_term = Some(26);

                    cfg.group_ops_bls12381_pairing_cost = Some(26897);
                    cfg.group_ops_bls12381_uncompressed_g1_sum_max_terms = Some(1200);

                    cfg.validator_validate_metadata_cost_base = Some(20000);
                }
                71 => {
                    cfg.sip_45_consensus_amplification_threshold = Some(5);

                    // Enable bursts for congestion control. (10x the per-commit budget)
                    cfg.allowed_txn_cost_overage_burst_per_object_in_commit = Some(185_000_000);
                }
                72 => {
                    cfg.feature_flags.convert_type_argument_error = true;

                    // Invariant: max_gas_price * base_tx_cost_fixed <= max_tx_gas
                    // max gas budget is in MIST and an absolute value 50_000 SUI
                    cfg.max_tx_gas = Some(50_000_000_000_000);
                    // max gas price is in MIST and an absolute value 50 SUI
                    cfg.max_gas_price = Some(50_000_000_000);

                    cfg.feature_flags.variant_nodes = true;
                }
                73 => {
                    // Enable new marker table version.
                    cfg.use_object_per_epoch_marker_table_v2 = Some(true);

                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        // Assuming a round rate of max 15/sec, then using a gc depth of 60 allow blocks within a window of ~4 seconds
                        // to be included before be considered garbage collected.
                        cfg.consensus_gc_depth = Some(60);
                    }

                    if chain != Chain::Mainnet {
                        // Enable zstd compression for consensus in testnet
                        cfg.feature_flags.consensus_zstd_compression = true;
                    }

                    // Enable smart ancestor selection for mainnet
                    cfg.feature_flags.consensus_smart_ancestor_selection = true;
                    // Enable probing for accepted rounds in round prober for mainnet
                    cfg.feature_flags
                        .consensus_round_prober_probe_accepted_rounds = true;

                    // Increase congestion control budget.
                    cfg.feature_flags.per_object_congestion_control_mode =
                        PerObjectCongestionControlMode::TotalGasBudgetWithCap;
                    cfg.gas_budget_based_txn_cost_cap_factor = Some(400_000);
                    cfg.max_accumulated_txn_cost_per_object_in_mysticeti_commit = Some(37_000_000);
                    cfg.max_accumulated_randomness_txn_cost_per_object_in_mysticeti_commit =
                        Some(7_400_000); // 20% of above
                    cfg.max_txn_cost_overage_per_object_in_commit = Some(u64::MAX);
                    cfg.gas_budget_based_txn_cost_absolute_cap_commit_count = Some(50);
                    cfg.allowed_txn_cost_overage_burst_per_object_in_commit = Some(370_000_000);
                }
                74 => {
                    // Enable nitro attestation verify native move function for devnet
                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.enable_nitro_attestation = true;
                    }
                    cfg.nitro_attestation_parse_base_cost = Some(53 * 50);
                    cfg.nitro_attestation_parse_cost_per_byte = Some(50);
                    cfg.nitro_attestation_verify_base_cost = Some(49632 * 50);
                    cfg.nitro_attestation_verify_cost_per_cert = Some(52369 * 50);

                    // Enable zstd compression for consensus in mainnet
                    cfg.feature_flags.consensus_zstd_compression = true;

                    if chain != Chain::Mainnet && chain != Chain::Testnet {
                        cfg.feature_flags.consensus_linearize_subdag_v2 = true;
                    }
                }
                // Use this template when making changes:
                //
                //     // modify an existing constant.
                //     move_binary_format_version: Some(7),
                //
                //     // Add a new constant (which is set to None in prior versions).
                //     new_constant: Some(new_value),
                //
                //     // Remove a constant (ensure that it is never accessed during this version).
                //     max_move_object_size: None,
                _ => panic!("unsupported version {:?}", version),
            }
        }
        cfg
    }

    // Extract the bytecode verifier config from this protocol config. `for_signing` indicates
    // whether this config is used for verification during signing or execution.
    pub fn verifier_config(&self, signing_limits: Option<(usize, usize)>) -> VerifierConfig {
        let (max_back_edges_per_function, max_back_edges_per_module) = if let Some((
            max_back_edges_per_function,
            max_back_edges_per_module,
        )) = signing_limits
        {
            (
                Some(max_back_edges_per_function),
                Some(max_back_edges_per_module),
            )
        } else {
            (None, None)
        };

        VerifierConfig {
            max_loop_depth: Some(self.max_loop_depth() as usize),
            max_generic_instantiation_length: Some(self.max_generic_instantiation_length() as usize),
            max_function_parameters: Some(self.max_function_parameters() as usize),
            max_basic_blocks: Some(self.max_basic_blocks() as usize),
            max_value_stack_size: self.max_value_stack_size() as usize,
            max_type_nodes: Some(self.max_type_nodes() as usize),
            max_push_size: Some(self.max_push_size() as usize),
            max_dependency_depth: Some(self.max_dependency_depth() as usize),
            max_fields_in_struct: Some(self.max_fields_in_struct() as usize),
            max_function_definitions: Some(self.max_function_definitions() as usize),
            max_data_definitions: Some(self.max_struct_definitions() as usize),
            max_constant_vector_len: Some(self.max_move_vector_len()),
            max_back_edges_per_function,
            max_back_edges_per_module,
            max_basic_blocks_in_script: None,
            max_idenfitier_len: self.max_move_identifier_len_as_option(), // Before protocol version 9, there was no limit
            allow_receiving_object_id: self.allow_receiving_object_id(),
            reject_mutable_random_on_entry_functions: self
                .reject_mutable_random_on_entry_functions(),
            bytecode_version: self.move_binary_format_version(),
            max_variants_in_enum: self.max_move_enum_variants_as_option(),
        }
    }

    /// Override one or more settings in the config, for testing.
    /// This must be called at the beginning of the test, before get_for_(min|max)_version is
    /// called, since those functions cache their return value.
    pub fn apply_overrides_for_testing(
        override_fn: impl Fn(ProtocolVersion, Self) -> Self + Send + 'static,
    ) -> OverrideGuard {
        CONFIG_OVERRIDE.with(|ovr| {
            let mut cur = ovr.borrow_mut();
            assert!(cur.is_none(), "config override already present");
            *cur = Some(Box::new(override_fn));
            OverrideGuard
        })
    }
}

// Setters for tests.
// This is only needed for feature_flags. Please suffix each setter with `_for_testing`.
// Non-feature_flags should already have test setters defined through macros.
impl ProtocolConfig {
    pub fn set_advance_to_highest_supported_protocol_version_for_testing(&mut self, val: bool) {
        self.feature_flags
            .advance_to_highest_supported_protocol_version = val
    }
    pub fn set_commit_root_state_digest_supported_for_testing(&mut self, val: bool) {
        self.feature_flags.commit_root_state_digest = val
    }
    pub fn set_zklogin_auth_for_testing(&mut self, val: bool) {
        self.feature_flags.zklogin_auth = val
    }
    pub fn set_enable_jwk_consensus_updates_for_testing(&mut self, val: bool) {
        self.feature_flags.enable_jwk_consensus_updates = val
    }
    pub fn set_random_beacon_for_testing(&mut self, val: bool) {
        self.feature_flags.random_beacon = val
    }

    pub fn set_upgraded_multisig_for_testing(&mut self, val: bool) {
        self.feature_flags.upgraded_multisig_supported = val
    }
    pub fn set_accept_zklogin_in_multisig_for_testing(&mut self, val: bool) {
        self.feature_flags.accept_zklogin_in_multisig = val
    }

    pub fn set_shared_object_deletion_for_testing(&mut self, val: bool) {
        self.feature_flags.shared_object_deletion = val;
    }

    pub fn set_narwhal_new_leader_election_schedule_for_testing(&mut self, val: bool) {
        self.feature_flags.narwhal_new_leader_election_schedule = val;
    }

    pub fn set_receive_object_for_testing(&mut self, val: bool) {
        self.feature_flags.receive_objects = val
    }
    pub fn set_narwhal_certificate_v2_for_testing(&mut self, val: bool) {
        self.feature_flags.narwhal_certificate_v2 = val
    }
    pub fn set_verify_legacy_zklogin_address_for_testing(&mut self, val: bool) {
        self.feature_flags.verify_legacy_zklogin_address = val
    }

    pub fn set_per_object_congestion_control_mode_for_testing(
        &mut self,
        val: PerObjectCongestionControlMode,
    ) {
        self.feature_flags.per_object_congestion_control_mode = val;
    }

    pub fn set_consensus_choice_for_testing(&mut self, val: ConsensusChoice) {
        self.feature_flags.consensus_choice = val;
    }

    pub fn set_consensus_network_for_testing(&mut self, val: ConsensusNetwork) {
        self.feature_flags.consensus_network = val;
    }

    pub fn set_zklogin_max_epoch_upper_bound_delta_for_testing(&mut self, val: Option<u64>) {
        self.feature_flags.zklogin_max_epoch_upper_bound_delta = val
    }

    pub fn set_disable_bridge_for_testing(&mut self) {
        self.feature_flags.bridge = false
    }

    pub fn set_mysticeti_num_leaders_per_round_for_testing(&mut self, val: Option<usize>) {
        self.feature_flags.mysticeti_num_leaders_per_round = val;
    }

    pub fn set_enable_soft_bundle_for_testing(&mut self, val: bool) {
        self.feature_flags.soft_bundle = val;
    }

    pub fn set_passkey_auth_for_testing(&mut self, val: bool) {
        self.feature_flags.passkey_auth = val
    }

    pub fn set_consensus_distributed_vote_scoring_strategy_for_testing(&mut self, val: bool) {
        self.feature_flags
            .consensus_distributed_vote_scoring_strategy = val;
    }

    pub fn set_consensus_round_prober_for_testing(&mut self, val: bool) {
        self.feature_flags.consensus_round_prober = val;
    }

    pub fn set_disallow_new_modules_in_deps_only_packages_for_testing(&mut self, val: bool) {
        self.feature_flags
            .disallow_new_modules_in_deps_only_packages = val;
    }

    pub fn set_consensus_round_prober_probe_accepted_rounds(&mut self, val: bool) {
        self.feature_flags
            .consensus_round_prober_probe_accepted_rounds = val;
    }

    pub fn set_consensus_linearize_subdag_v2_for_testing(&mut self, val: bool) {
        self.feature_flags.consensus_linearize_subdag_v2 = val;
    }

    pub fn set_mysticeti_fastpath_for_testing(&mut self, val: bool) {
        self.feature_flags.mysticeti_fastpath = val;
    }
}

type OverrideFn = dyn Fn(ProtocolVersion, ProtocolConfig) -> ProtocolConfig + Send;

thread_local! {
    static CONFIG_OVERRIDE: RefCell<Option<Box<OverrideFn>>> = RefCell::new(None);
}

#[must_use]
pub struct OverrideGuard;

impl Drop for OverrideGuard {
    fn drop(&mut self) {
        info!("restoring override fn");
        CONFIG_OVERRIDE.with(|ovr| {
            *ovr.borrow_mut() = None;
        });
    }
}

/// Defines which limit got crossed.
/// The value which crossed the limit and value of the limit crossed are embedded
#[derive(PartialEq, Eq)]
pub enum LimitThresholdCrossed {
    None,
    Soft(u128, u128),
    Hard(u128, u128),
}

/// Convenience function for comparing limit ranges
/// V::MAX must be at >= U::MAX and T::MAX
pub fn check_limit_in_range<T: Into<V>, U: Into<V>, V: PartialOrd + Into<u128>>(
    x: T,
    soft_limit: U,
    hard_limit: V,
) -> LimitThresholdCrossed {
    let x: V = x.into();
    let soft_limit: V = soft_limit.into();

    debug_assert!(soft_limit <= hard_limit);

    // It is important to preserve this comparison order because if soft_limit == hard_limit
    // we want LimitThresholdCrossed::Hard
    if x >= hard_limit {
        LimitThresholdCrossed::Hard(x.into(), hard_limit.into())
    } else if x < soft_limit {
        LimitThresholdCrossed::None
    } else {
        LimitThresholdCrossed::Soft(x.into(), soft_limit.into())
    }
}

#[macro_export]
macro_rules! check_limit {
    ($x:expr, $hard:expr) => {
        check_limit!($x, $hard, $hard)
    };
    ($x:expr, $soft:expr, $hard:expr) => {
        check_limit_in_range($x as u64, $soft, $hard)
    };
}

/// Used to check which limits were crossed if the TX is metered (not system tx)
/// Args are: is_metered, value_to_check, metered_limit, unmetered_limit
/// metered_limit is always less than or equal to unmetered_hard_limit
#[macro_export]
macro_rules! check_limit_by_meter {
    ($is_metered:expr, $x:expr, $metered_limit:expr, $unmetered_hard_limit:expr, $metric:expr) => {{
        // If this is metered, we use the metered_limit limit as the upper bound
        let (h, metered_str) = if $is_metered {
            ($metered_limit, "metered")
        } else {
            // Unmetered gets more headroom
            ($unmetered_hard_limit, "unmetered")
        };
        use sui_protocol_config::check_limit_in_range;
        let result = check_limit_in_range($x as u64, $metered_limit, h);
        match result {
            LimitThresholdCrossed::None => {}
            LimitThresholdCrossed::Soft(_, _) => {
                $metric.with_label_values(&[metered_str, "soft"]).inc();
            }
            LimitThresholdCrossed::Hard(_, _) => {
                $metric.with_label_values(&[metered_str, "hard"]).inc();
            }
        };
        result
    }};
}

pub fn is_mysticeti_fpc_enabled_in_env() -> Option<bool> {
    if let Ok(v) = std::env::var("CONSENSUS") {
        if v == "mysticeti_fpc" {
            return Some(true);
        } else if v == "mysticeti" {
            return Some(false);
        }
    }
    None
}

#[cfg(all(test, not(msim)))]
mod test {
    use insta::assert_yaml_snapshot;

    use super::*;

    #[test]
    fn snapshot_tests() {
        println!("\n============================================================================");
        println!("!                                                                          !");
        println!("! IMPORTANT: never update snapshots from this test. only add new versions! !");
        println!("!                                                                          !");
        println!("============================================================================\n");
        for chain_id in &[Chain::Unknown, Chain::Mainnet, Chain::Testnet] {
            // make Chain::Unknown snapshots compatible with pre-chain-id snapshots so that we
            // don't break the release-time compatibility tests. Once Chain Id configs have been
            // released everywhere, we can remove this and only test Mainnet and Testnet
            let chain_str = match chain_id {
                Chain::Unknown => "".to_string(),
                _ => format!("{:?}_", chain_id),
            };
            for i in MIN_PROTOCOL_VERSION..=MAX_PROTOCOL_VERSION {
                let cur = ProtocolVersion::new(i);
                assert_yaml_snapshot!(
                    format!("{}version_{}", chain_str, cur.as_u64()),
                    ProtocolConfig::get_for_version(cur, *chain_id)
                );
            }
        }
    }

    #[test]
    fn test_getters() {
        let prot: ProtocolConfig =
            ProtocolConfig::get_for_version(ProtocolVersion::new(1), Chain::Unknown);
        assert_eq!(
            prot.max_arguments(),
            prot.max_arguments_as_option().unwrap()
        );
    }

    #[test]
    fn test_setters() {
        let mut prot: ProtocolConfig =
            ProtocolConfig::get_for_version(ProtocolVersion::new(1), Chain::Unknown);
        prot.set_max_arguments_for_testing(123);
        assert_eq!(prot.max_arguments(), 123);

        prot.set_max_arguments_from_str_for_testing("321".to_string());
        assert_eq!(prot.max_arguments(), 321);

        prot.disable_max_arguments_for_testing();
        assert_eq!(prot.max_arguments_as_option(), None);

        prot.set_attr_for_testing("max_arguments".to_string(), "456".to_string());
        assert_eq!(prot.max_arguments(), 456);
    }

    #[test]
    #[should_panic(expected = "unsupported version")]
    fn max_version_test() {
        // When this does not panic, version higher than MAX_PROTOCOL_VERSION exists.
        // To fix, bump MAX_PROTOCOL_VERSION or disable this check for the version.
        let _ = ProtocolConfig::get_for_version_impl(
            ProtocolVersion::new(MAX_PROTOCOL_VERSION + 1),
            Chain::Unknown,
        );
    }

    #[test]
    fn lookup_by_string_test() {
        let prot: ProtocolConfig =
            ProtocolConfig::get_for_version(ProtocolVersion::new(1), Chain::Unknown);
        // Does not exist
        assert!(prot.lookup_attr("some random string".to_string()).is_none());

        assert!(
            prot.lookup_attr("max_arguments".to_string())
                == Some(ProtocolConfigValue::u32(prot.max_arguments())),
        );

        // We didnt have this in version 1
        assert!(prot
            .lookup_attr("max_move_identifier_len".to_string())
            .is_none());

        // But we did in version 9
        let prot: ProtocolConfig =
            ProtocolConfig::get_for_version(ProtocolVersion::new(9), Chain::Unknown);
        assert!(
            prot.lookup_attr("max_move_identifier_len".to_string())
                == Some(ProtocolConfigValue::u64(prot.max_move_identifier_len()))
        );

        let prot: ProtocolConfig =
            ProtocolConfig::get_for_version(ProtocolVersion::new(1), Chain::Unknown);
        // We didnt have this in version 1
        assert!(prot
            .attr_map()
            .get("max_move_identifier_len")
            .unwrap()
            .is_none());
        // We had this in version 1
        assert!(
            prot.attr_map().get("max_arguments").unwrap()
                == &Some(ProtocolConfigValue::u32(prot.max_arguments()))
        );

        // Check feature flags
        let prot: ProtocolConfig =
            ProtocolConfig::get_for_version(ProtocolVersion::new(1), Chain::Unknown);
        // Does not exist
        assert!(prot
            .feature_flags
            .lookup_attr("some random string".to_owned())
            .is_none());
        assert!(!prot
            .feature_flags
            .attr_map()
            .contains_key("some random string"));

        // Was false in v1
        assert!(
            prot.feature_flags
                .lookup_attr("package_upgrades".to_owned())
                == Some(false)
        );
        assert!(
            prot.feature_flags
                .attr_map()
                .get("package_upgrades")
                .unwrap()
                == &false
        );
        let prot: ProtocolConfig =
            ProtocolConfig::get_for_version(ProtocolVersion::new(4), Chain::Unknown);
        // Was true from v3 and up
        assert!(
            prot.feature_flags
                .lookup_attr("package_upgrades".to_owned())
                == Some(true)
        );
        assert!(
            prot.feature_flags
                .attr_map()
                .get("package_upgrades")
                .unwrap()
                == &true
        );
    }

    #[test]
    fn limit_range_fn_test() {
        let low = 100u32;
        let high = 10000u64;

        assert!(check_limit!(1u8, low, high) == LimitThresholdCrossed::None);
        assert!(matches!(
            check_limit!(255u16, low, high),
            LimitThresholdCrossed::Soft(255u128, 100)
        ));
        // This wont compile because lossy
        //assert!(check_limit!(100000000u128, low, high) == LimitThresholdCrossed::None);
        // This wont compile because lossy
        //assert!(check_limit!(100000000usize, low, high) == LimitThresholdCrossed::None);

        assert!(matches!(
            check_limit!(2550000u64, low, high),
            LimitThresholdCrossed::Hard(2550000, 10000)
        ));

        assert!(matches!(
            check_limit!(2550000u64, high, high),
            LimitThresholdCrossed::Hard(2550000, 10000)
        ));

        assert!(matches!(
            check_limit!(1u8, high),
            LimitThresholdCrossed::None
        ));

        assert!(check_limit!(255u16, high) == LimitThresholdCrossed::None);

        assert!(matches!(
            check_limit!(2550000u64, high),
            LimitThresholdCrossed::Hard(2550000, 10000)
        ));
    }
}
