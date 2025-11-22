// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::validator_tests;

use std::unit_test::{assert_eq, destroy};
use sui::balance;
use sui::url;
use sui_system::staking_pool::StakedSui;
use sui_system::test_runner;
use sui_system::validator_builder;

const DEFAULT_STAKE: u64 = 10;

#[test]
// Scenario:
// 1. Create a validator with initial stake
// 2. Check that the validator has the correct initial stake
// 3. Destroy the validator
fun validator_owner_flow() {
    let initial_stake = DEFAULT_STAKE * 1_000_000_000;
    let mut runner = test_runner::new().build();
    runner.set_sender(@2);

    let validator = validator_builder::new()
        .sui_address(@2)
        .initial_stake(DEFAULT_STAKE)
        .build(runner.ctx());

    let pool_id = validator.staking_pool_id();

    assert_eq!(validator.total_stake(), initial_stake);
    assert_eq!(validator.sui_address(), @2);
    test_runner::destroy(validator);

    runner.owned_tx!<StakedSui>(|stake| {
        assert_eq!(stake.amount(), initial_stake);
        assert_eq!(stake.pool_id(), pool_id);
        assert_eq!(stake.stake_activation_epoch(), 0);
        runner.keep(stake);
    });

    runner.finish();
}

#[test]
// Scenario:
// 1. Create a validator with initial stake
// 2. Add extra stake, check pending stake amount
// 3. Withdraw initial stake, check pending stake amount and pending withdraw amount
// 4. Trigger state change and pending processing
// 5. Check that pending values have been processed
fun pending_validator_flow() {
    let initial_stake = DEFAULT_STAKE * 1_000_000_000;
    let added_stake = 30_000_000_000;
    let mut runner = test_runner::new().build();
    runner.set_sender(@2);

    let mut validator = validator_builder::new()
        .sui_address(@2)
        .initial_stake(DEFAULT_STAKE)
        .is_active_at_genesis(true)
        .build(runner.ctx());

    // add extra stake, but don't send it to the inventory just yet
    let extra_stake = validator.request_add_stake(test_runner::mint(30), @2, runner.ctx());

    assert_eq!(validator.total_stake(), initial_stake);
    assert_eq!(validator.pending_stake_amount(), added_stake);

    // take initial stake out of inventory
    runner.owned_tx!<StakedSui>(|staked_sui| {
        let withdrawn_balance = validator
            .request_withdraw_stake(staked_sui, runner.ctx())
            .destroy_for_testing();

        assert_eq!(withdrawn_balance, initial_stake);
        assert_eq!(validator.total_stake(), initial_stake);
        assert_eq!(validator.pending_stake_amount(), added_stake);
        assert_eq!(validator.pending_stake_withdraw_amount(), initial_stake);

        // trigger the state change and pending processing
        validator.deposit_stake_rewards(balance::zero());
        validator.process_pending_stakes_and_withdraws(runner.ctx());

        assert_eq!(validator.total_stake(), added_stake);
        assert_eq!(validator.pending_stake_amount(), 0);
        assert_eq!(validator.pending_stake_withdraw_amount(), 0);
    });

    runner.finish();
    test_runner::destroy(validator);
    test_runner::destroy(extra_stake);
}

#[test]
fun metadata() {
    let ctx = &mut tx_context::dummy();
    let metadata = validator_builder::preset().build_metadata(ctx);
    metadata.validate();
    destroy(metadata);
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidPubkey)]
fun metadata_invalid_pubkey() {
    let ctx = &mut tx_context::dummy();
    let metadata = validator_builder::preset()
        .protocol_pubkey_bytes(b"incorrect")
        .build_metadata(ctx);

    metadata.validate();

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidNetPubkey)]
fun metadata_invalid_net_pubkey() {
    let ctx = &mut tx_context::dummy();
    let metadata = validator_builder::preset()
        .network_pubkey_bytes(b"incorrect")
        .build_metadata(ctx);

    metadata.validate();

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidWorkerPubkey)]
fun metadata_invalid_worker_pubkey() {
    let ctx = &mut tx_context::dummy();
    let metadata = validator_builder::preset()
        .worker_pubkey_bytes(b"incorrect")
        .build_metadata(ctx);

    metadata.validate();

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidNetAddr)]
fun metadata_invalid_net_addr() {
    let ctx = &mut tx_context::dummy();
    let metadata = validator_builder::preset().net_address(b"incorrect").build_metadata(ctx);

    metadata.validate();

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidP2pAddr)]
fun metadata_invalid_p2p_addr() {
    let ctx = &mut tx_context::dummy();
    let metadata = validator_builder::preset().p2p_address(b"incorrect").build_metadata(ctx);

    metadata.validate();

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidPrimaryAddr)]
fun metadata_invalid_consensus_addr() {
    let ctx = &mut tx_context::dummy();
    let metadata = validator_builder::preset().primary_address(b"incorrect").build_metadata(ctx);

    metadata.validate();

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidWorkerAddr)]
fun metadata_invalid_worker_addr() {
    let ctx = &mut tx_context::dummy();
    let metadata = validator_builder::preset().worker_address(b"incorrect").build_metadata(ctx);

    metadata.validate();

    abort
}

#[test, allow(implicit_const_copy)]
fun validator_update_metadata_ok() {
    let new_protocol_pub_key =
        x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c";
    let new_pop =
        x"a8a0bcaf04e13565914eb22fa9f27a76f297db04446860ee2b923d10224cedb130b30783fb60b12556e7fc50e5b57a86";

    // prettier-ignore
    let new_worker_pub_key = vector[115, 220, 238, 151, 134, 159, 173, 41, 80, 2, 66, 196, 61, 17, 191, 76, 103, 39, 246, 127, 171, 85, 19, 235, 210, 106, 97, 97, 116, 48, 244, 191];
    // prettier-ignore
    let new_network_pub_key = vector[149, 128, 161, 13, 11, 183, 96, 45, 89, 20, 188, 205, 26, 127, 147, 254, 184, 229, 184, 102, 64, 170, 104, 29, 191, 171, 91, 99, 58, 178, 41, 156];

    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    // perform updates
    validator.update_next_epoch_network_address(b"/ip4/192.168.1.1/tcp/80");
    validator.update_next_epoch_p2p_address(b"/ip4/192.168.1.1/udp/80");
    validator.update_next_epoch_primary_address(b"/ip4/192.168.1.1/udp/80");
    validator.update_next_epoch_worker_address(b"/ip4/192.168.1.1/udp/80");
    validator.update_next_epoch_protocol_pubkey(new_protocol_pub_key, new_pop);
    validator.update_next_epoch_worker_pubkey(new_worker_pub_key);
    validator.update_next_epoch_network_pubkey(new_network_pub_key);

    validator.update_name(b"new_name");
    validator.update_description(b"new_desc");
    validator.update_image_url(b"new_image_url");
    validator.update_project_url(b"new_proj_url");

    // check updates
    assert_eq!(*validator.name(), b"new_name".to_string());
    assert_eq!(*validator.description(), b"new_desc".to_string());
    assert_eq!(*validator.image_url(), url::new_unsafe_from_bytes(b"new_image_url"));
    assert_eq!(*validator.project_url(), url::new_unsafe_from_bytes(b"new_proj_url"));
    assert_eq!(*validator.network_address(), validator_builder::valid_net_addr().to_string());
    assert_eq!(*validator.p2p_address(), validator_builder::valid_p2p_addr().to_string());
    assert_eq!(*validator.primary_address(), validator_builder::valid_consensus_addr().to_string());
    assert_eq!(*validator.worker_address(), validator_builder::valid_worker_addr().to_string());
    assert_eq!(*validator.protocol_pubkey_bytes(), validator_builder::valid_pubkey());
    assert_eq!(*validator.proof_of_possession(), validator_builder::valid_proof_of_possession());
    assert_eq!(*validator.network_pubkey_bytes(), validator_builder::valid_net_pubkey());
    assert_eq!(*validator.worker_pubkey_bytes(), validator_builder::valid_worker_pubkey());

    // next epoch
    assert!(
        validator
            .next_epoch_network_address()
            .is_some_and!(|addr| addr == b"/ip4/192.168.1.1/tcp/80".to_string()),
    );
    assert!(
        validator
            .next_epoch_p2p_address()
            .is_some_and!(|addr| addr == b"/ip4/192.168.1.1/udp/80".to_string()),
    );
    assert!(
        validator
            .next_epoch_primary_address()
            .is_some_and!(|addr| addr == b"/ip4/192.168.1.1/udp/80".to_string()),
    );
    assert!(
        validator
            .next_epoch_worker_address()
            .is_some_and!(|addr| addr == b"/ip4/192.168.1.1/udp/80".to_string()),
    );
    assert!(
        validator
            .next_epoch_protocol_pubkey_bytes()
            .is_some_and!(|key| key == new_protocol_pub_key),
    );
    assert!(validator.next_epoch_proof_of_possession().is_some_and!(|pop| pop == new_pop));
    assert!(
        validator.next_epoch_worker_pubkey_bytes().is_some_and!(|key| key == new_worker_pub_key),
    );
    assert!(
        validator.next_epoch_network_pubkey_bytes().is_some_and!(|key| key == new_network_pub_key),
    );

    destroy(validator);
}

#[test, expected_failure(abort_code = sui_system::validator::EInvalidProofOfPossession)]
fun validator_update_metadata_invalid_proof_of_possession() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_protocol_pubkey(
        x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c",
        x"8b9794dfd11b88e16ba8f6a4a2c1e7580738dce2d6910ee594bebd88297b22ae8c34d1ee3f5a081159d68e076ef5d300",
    );

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidNetPubkey)]
fun validator_update_metadata_invalid_network_key() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_network_pubkey(x"beef");

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidWorkerPubkey)]
fun validator_update_metadata_invalid_worker_key() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_worker_pubkey(x"beef");

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidNetAddr)]
fun validator_update_metadata_invalid_network_addr() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_network_address(b"beef");

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidPrimaryAddr)]
fun validator_update_metadata_invalid_primary_addr() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_primary_address(b"beef");

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidWorkerAddr)]
fun validator_update_metadata_invalid_worker_addr() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_worker_address(b"beef");

    abort
}

#[test, expected_failure(abort_code = sui_system::validator::EMetadataInvalidP2pAddr)]
fun validator_update_metadata_invalid_p2p_address() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_p2p_address(b"beef");

    abort
}

#[
    test,
    expected_failure(
        abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit,
    ),
]
fun validator_update_metadata_primary_address_too_long() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_primary_address(vector::tabulate!(257, |_| 0));
    abort
}

#[
    test,
    expected_failure(
        abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit,
    ),
]
fun validator_update_metadata_net_address_too_long() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_network_address(vector::tabulate!(257, |_| 0));

    abort
}

#[
    test,
    expected_failure(
        abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit,
    ),
]
fun validator_update_metadata_worker_address_too_long() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_worker_address(vector::tabulate!(257, |_| 0));

    abort
}

#[
    test,
    expected_failure(
        abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit,
    ),
]
fun validator_update_metadata_p2p_address_too_long() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_next_epoch_p2p_address(vector::tabulate!(257, |_| 0));

    abort
}

#[
    test,
    expected_failure(
        abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit,
    ),
]
fun validator_update_name_too_long() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_name(vector::tabulate!(257, |_| 0));

    abort
}

#[
    test,
    expected_failure(
        abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit,
    ),
]
fun validator_update_description_too_long() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_description(vector::tabulate!(257, |_| 0));

    abort
}

#[
    test,
    expected_failure(
        abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit,
    ),
]
fun validator_update_project_url_too_long() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_project_url(vector::tabulate!(257, |_| 0));

    abort
}

#[
    test,
    expected_failure(
        abort_code = sui_system::validator::EValidatorMetadataExceedingLengthLimit,
    ),
]
fun validator_update_image_url_too_long() {
    let ctx = &mut tx_context::dummy();
    let mut validator = validator_builder::preset().build(ctx);

    validator.update_image_url(vector::tabulate!(257, |_| 0));

    abort
}
