// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for ProgrammableTransaction::structural_digest (SIP-70).
//!
//! Verifies that the structural digest is:
//! - Deterministic (same PTB → same digest)
//! - Sensitive to command addition/removal/reordering
//! - Sensitive to argument changes (different Pure values, different objects)
//! - Sensitive to Result redirection (changing which command output flows where)
//! - Stable when only irrelevant metadata changes (e.g. object digest bytes)

use crate::base_types::{ObjectID, SequenceNumber, ObjectDigest, random_object_ref};
use crate::transaction::{
    Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, ProgrammableTransaction,
};
use crate::type_input::TypeInput;

/// Helper: build a simple MoveCall command
fn make_move_call(
    package: ObjectID,
    module: &str,
    function: &str,
    arguments: Vec<Argument>,
) -> Command {
    Command::MoveCall(Box::new(ProgrammableMoveCall {
        package,
        module: module.to_string(),
        function: function.to_string(),
        type_arguments: vec![],
        arguments,
    }))
}

/// Helper: build a ProgrammableTransaction from inputs and commands
fn make_pt(inputs: Vec<CallArg>, commands: Vec<Command>) -> ProgrammableTransaction {
    ProgrammableTransaction { inputs, commands }
}

// ============================================================================
// Determinism
// ============================================================================

#[test]
fn test_structural_digest_is_deterministic() {
    let obj_ref = random_object_ref();
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref))],
        vec![make_move_call(pkg, "module", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref))],
        vec![make_move_call(pkg, "module", "func", vec![Argument::Input(0)])],
    );

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_structural_digest_length() {
    let pt = make_pt(vec![], vec![]);
    let digest = pt.structural_digest();
    // Blake2b256 produces 32 bytes
    assert_eq!(digest.len(), 32);
}

// ============================================================================
// Sensitivity to commands
// ============================================================================

#[test]
fn test_digest_changes_with_extra_command() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Pure(bcs::to_bytes(&100u64).unwrap())],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Pure(bcs::to_bytes(&100u64).unwrap())],
        vec![
            make_move_call(pkg, "mod", "func", vec![Argument::Input(0)]),
            make_move_call(pkg, "mod", "func2", vec![Argument::Result(0)]),
        ],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_command_reorder() {
    let pkg = ObjectID::random();

    let cmd_a = make_move_call(pkg, "mod", "func_a", vec![]);
    let cmd_b = make_move_call(pkg, "mod", "func_b", vec![]);

    let pt1 = make_pt(vec![], vec![cmd_a.clone(), cmd_b.clone()]);
    let pt2 = make_pt(vec![], vec![cmd_b, cmd_a]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_command_removal() {
    let pkg = ObjectID::random();

    let cmd_a = make_move_call(pkg, "mod", "func_a", vec![]);
    let cmd_b = make_move_call(pkg, "mod", "func_b", vec![]);

    let pt1 = make_pt(vec![], vec![cmd_a.clone(), cmd_b]);
    let pt2 = make_pt(vec![], vec![cmd_a]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Sensitivity to arguments
// ============================================================================

#[test]
fn test_digest_changes_with_different_pure_value() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Pure(bcs::to_bytes(&100u64).unwrap())],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Pure(bcs::to_bytes(&200u64).unwrap())],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_different_shared_object() {
    let pkg = ObjectID::random();
    let shared_id_1 = ObjectID::random();
    let shared_id_2 = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: shared_id_1,
            initial_shared_version: SequenceNumber::new(),
            mutability: crate::transaction::SharedObjectMutability::Mutable,
        })],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: shared_id_2,
            initial_shared_version: SequenceNumber::new(),
            mutability: crate::transaction::SharedObjectMutability::Mutable,
        })],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_stable_across_shared_object_version_change() {
    // Shared objects are hashed by ObjectID only, not version.
    // So changing initial_shared_version should NOT change the digest.
    let pkg = ObjectID::random();
    let shared_id = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: shared_id,
            initial_shared_version: SequenceNumber::from_u64(1),
            mutability: crate::transaction::SharedObjectMutability::Mutable,
        })],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: shared_id,
            initial_shared_version: SequenceNumber::from_u64(999),
            mutability: crate::transaction::SharedObjectMutability::Mutable,
        })],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_stable_across_object_digest_change() {
    // Owned objects are hashed by ObjectID + version. The ObjectDigest
    // (content hash) is NOT included, so changing it should not affect the digest.
    let pkg = ObjectID::random();
    let obj_id = ObjectID::random();
    let version = SequenceNumber::from_u64(5);

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            obj_id,
            version,
            ObjectDigest::new([0xAA; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject((
            obj_id,
            version,
            ObjectDigest::new([0xBB; 32]),
        )))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_different_owned_object() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(random_object_ref()))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    let pt2 = make_pt(
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(random_object_ref()))],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Sensitivity to Result redirection (provenance)
// ============================================================================

#[test]
fn test_digest_changes_with_result_redirection() {
    let pkg = ObjectID::random();

    // PTB1: cmd0 produces result, cmd1 uses Result(0)
    let pt1 = make_pt(
        vec![],
        vec![
            make_move_call(pkg, "mod", "produce", vec![]),
            make_move_call(pkg, "mod", "consume", vec![Argument::Result(0)]),
        ],
    );

    // PTB2: cmd0 and cmd1 produce results, cmd2 uses Result(1) instead of Result(0)
    let pt2 = make_pt(
        vec![],
        vec![
            make_move_call(pkg, "mod", "produce", vec![]),
            make_move_call(pkg, "mod", "produce2", vec![]),
            make_move_call(pkg, "mod", "consume", vec![Argument::Result(1)]),
        ],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_distinguishes_result_vs_nested_result() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![],
        vec![
            make_move_call(pkg, "mod", "produce", vec![]),
            make_move_call(pkg, "mod", "consume", vec![Argument::Result(0)]),
        ],
    );

    let pt2 = make_pt(
        vec![],
        vec![
            make_move_call(pkg, "mod", "produce", vec![]),
            make_move_call(pkg, "mod", "consume", vec![Argument::NestedResult(0, 0)]),
        ],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// GasCoin normalization
// ============================================================================

#[test]
fn test_digest_gas_coin_is_stable() {
    let pkg = ObjectID::random();

    // Two identical PTBs using GasCoin — should produce same digest
    let pt1 = make_pt(
        vec![],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::GasCoin])],
    );

    let pt2 = make_pt(
        vec![],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::GasCoin])],
    );

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_gas_coin_differs_from_input() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::GasCoin])],
    );

    let pt2 = make_pt(
        vec![CallArg::Pure(vec![0x00])],
        vec![make_move_call(pkg, "mod", "func", vec![Argument::Input(0)])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Command type sensitivity
// ============================================================================

#[test]
fn test_digest_distinguishes_command_types() {
    let obj_ref = random_object_ref();

    // SplitCoins vs MergeCoins with same arguments should differ
    let pt1 = make_pt(
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref)),
            CallArg::Pure(bcs::to_bytes(&100u64).unwrap()),
        ],
        vec![Command::SplitCoins(Argument::Input(0), vec![Argument::Input(1)])],
    );

    let pt2 = make_pt(
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref)),
            CallArg::Pure(bcs::to_bytes(&100u64).unwrap()),
        ],
        vec![Command::MergeCoins(Argument::Input(0), vec![Argument::Input(1)])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Target sensitivity
// ============================================================================

#[test]
fn test_digest_changes_with_different_package() {
    let pkg1 = ObjectID::random();
    let pkg2 = ObjectID::random();

    let pt1 = make_pt(
        vec![],
        vec![make_move_call(pkg1, "mod", "func", vec![])],
    );

    let pt2 = make_pt(
        vec![],
        vec![make_move_call(pkg2, "mod", "func", vec![])],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_different_module() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(vec![], vec![make_move_call(pkg, "mod_a", "func", vec![])]);
    let pt2 = make_pt(vec![], vec![make_move_call(pkg, "mod_b", "func", vec![])]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_digest_changes_with_different_function() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(vec![], vec![make_move_call(pkg, "mod", "func_a", vec![])]);
    let pt2 = make_pt(vec![], vec![make_move_call(pkg, "mod", "func_b", vec![])]);

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Type argument sensitivity
// ============================================================================

#[test]
fn test_digest_changes_with_different_type_arguments() {
    let pkg = ObjectID::random();

    let pt1 = make_pt(
        vec![],
        vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
            package: pkg,
            module: "mod".to_string(),
            function: "func".to_string(),
            type_arguments: vec![TypeInput::U64],
            arguments: vec![],
        }))],
    );

    let pt2 = make_pt(
        vec![],
        vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
            package: pkg,
            module: "mod".to_string(),
            function: "func".to_string(),
            type_arguments: vec![TypeInput::Bool],
            arguments: vec![],
        }))],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Empty PTB
// ============================================================================

#[test]
fn test_empty_ptb_has_stable_digest() {
    let pt1 = make_pt(vec![], vec![]);
    let pt2 = make_pt(vec![], vec![]);

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

// ============================================================================
// Complex multi-command PTB (governance scenario)
// ============================================================================

#[test]
fn test_governance_scenario_digest_stability() {
    // Simulate: SplitCoin → Swap → Deposit
    // The digest should be stable for the same logical flow
    let pkg = ObjectID::random();
    let coin_ref = random_object_ref();

    let build_ptb = || {
        make_pt(
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref)),
                CallArg::Pure(bcs::to_bytes(&1000u64).unwrap()),
            ],
            vec![
                // cmd 0: split coin
                Command::SplitCoins(Argument::Input(0), vec![Argument::Input(1)]),
                // cmd 1: swap (uses Result(0) = split output)
                make_move_call(pkg, "dex", "swap", vec![Argument::Result(0)]),
                // cmd 2: deposit (uses Result(1) = swap output)
                make_move_call(pkg, "vault", "deposit", vec![Argument::Result(1)]),
            ],
        )
    };

    let pt1 = build_ptb();
    let pt2 = build_ptb();

    assert_eq!(pt1.structural_digest(), pt2.structural_digest());
}

#[test]
fn test_governance_scenario_detects_swap_target_change() {
    // Same flow but swapping on a different DEX should change the digest
    let pkg1 = ObjectID::random();
    let pkg2 = ObjectID::random();
    let vault_pkg = ObjectID::random();
    let coin_ref = random_object_ref();

    let pt1 = make_pt(
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref)),
            CallArg::Pure(bcs::to_bytes(&1000u64).unwrap()),
        ],
        vec![
            Command::SplitCoins(Argument::Input(0), vec![Argument::Input(1)]),
            make_move_call(pkg1, "dex", "swap", vec![Argument::Result(0)]),
            make_move_call(vault_pkg, "vault", "deposit", vec![Argument::Result(1)]),
        ],
    );

    let pt2 = make_pt(
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(coin_ref)),
            CallArg::Pure(bcs::to_bytes(&1000u64).unwrap()),
        ],
        vec![
            Command::SplitCoins(Argument::Input(0), vec![Argument::Input(1)]),
            make_move_call(pkg2, "dex", "swap", vec![Argument::Result(0)]),
            make_move_call(vault_pkg, "vault", "deposit", vec![Argument::Result(1)]),
        ],
    );

    assert_ne!(pt1.structural_digest(), pt2.structural_digest());
}
