// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module sui_system::validator_metadata_tests;

use std::unit_test;
use sui::test_scenario::{Self, Scenario};
use sui::url;
use sui_system::sui_system::SuiSystemState;
use sui_system::test_runner;
use sui_system::validator::{Self, Validator};
use sui_system::validator_builder;
use sui_system::validator_preset;
use sui_system::validator_set;

#[test]
// Sanity check that the validator presets in the builder are valid and pass
// metadata validation.
fun metadata_presets() {
    let ctx = &mut tx_context::dummy();
    let validator_0 = validator_builder::preset(0).build(ctx);
    let validator_1 = validator_builder::preset(1).build(ctx);
    let validator_2 = validator_builder::preset(2).build(ctx);
    let validator_3 = validator_builder::preset(3).build(ctx);

    unit_test::destroy(vector[validator_0, validator_1, validator_2, validator_3]);
}

// === Is Duplicate Tests ===

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
// Validator 0 tries to set the name of validator 1.
// Metadata validation should fail.
fun cannot_set_duplicate_name() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_name(preset_1.name(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun cannot_set_duplicate_net_address() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_network_address(preset_1.net_address(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun cannot_set_duplicate_p2p_address() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_p2p_address(preset_1.p2p_address(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun cannot_set_duplicate_primary_address() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_primary_address(preset_1.primary_address(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun cannot_set_duplicate_worker_address() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_worker_address(preset_1.worker_address(), ctx);
    });

    abort
}

// TODO: come back to me
#[test, expected_failure(abort_code = 0, location = validator)]
fun cannot_set_duplicate_protocol_pubkey_bytes() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_protocol_pubkey(
            preset_1.protocol_pubkey_bytes(),
            preset_1.proof_of_possession(),
            ctx,
        );
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun cannot_set_duplicate_network_pubkey_bytes() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_network_pubkey(
            preset_1.network_pubkey_bytes(),
            ctx,
        );
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun cannot_set_duplicate_worker_pubkey_bytes() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_worker_pubkey(
            preset_1.worker_pubkey_bytes(),
            ctx,
        );
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun cannot_set_duplicate_next_epoch_net_address() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_network_address(preset_1.net_address(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun cannot_set_duplicate_next_epoch_p2p_address() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_p2p_address(preset_1.p2p_address(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun cannot_set_duplicate_next_epoch_primary_address() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    // Validator 1 sets a new next_epoch primary address.
    let new_primary = b"/ip4/99.99.99.99/udp/80";
    runner.set_sender(preset_1.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_primary_address(new_primary, ctx);
    });

    // Validator 0 tries to set the same next_epoch primary address.
    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_primary_address(new_primary, ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun cannot_set_duplicate_next_epoch_worker_address() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    // Validator 1 sets a new next_epoch worker address.
    let new_worker = b"/ip4/99.99.99.99/udp/81";
    runner.set_sender(preset_1.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_worker_address(new_worker, ctx);
    });

    // Validator 0 tries to set the same next_epoch worker address.
    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_worker_address(new_worker, ctx);
    });

    abort
}

// TODO: consider how to enable this test
#[test, expected_failure(abort_code = 0, location = validator)]
fun cannot_set_duplicate_next_epoch_protocol_pubkey_bytes() {
    let preset_0 = validator_preset::preset(0);
    let preset_1 = validator_preset::preset(1);
    let mut runner = test_runner::new()
        .validators(vector[
            validator_builder::from_preset(preset_0),
            validator_builder::from_preset(preset_1),
        ])
        .build();

    runner.set_sender(preset_0.account_address()).system_tx!(|system, ctx| {
        system.update_validator_next_epoch_protocol_pubkey(
            preset_1.protocol_pubkey_bytes(),
            preset_1.proof_of_possession(),
            ctx,
        );
    });

    abort
}

// === Candidate Validator Metadata Tests ===

#[test]
/// Scenario:
/// - create a candidate validator
/// - update all possible metadata fields for the candidate
fun candidate_validator_update_metadata() {
    let mut runner = test_runner::new().validators_count(1).build();

    let preset = validator_preset::preset(2);
    let candidate = validator_builder::preset(1).initial_stake(10000).build(runner.ctx());
    let candidate_addr = candidate.sui_address();

    runner.set_sender(candidate_addr).add_validator_candidate(candidate);
    runner.system_tx!(|system, ctx| {
        system.update_candidate_validator_network_address(preset.net_address(), ctx);
        system.update_candidate_validator_p2p_address(preset.p2p_address(), ctx);
        system.update_candidate_validator_primary_address(preset.primary_address(), ctx);
        system.update_candidate_validator_worker_address(preset.worker_address(), ctx);
        system.update_candidate_validator_network_pubkey(preset.network_pubkey_bytes(), ctx);
        system.update_candidate_validator_worker_pubkey(preset.worker_pubkey_bytes(), ctx);

        system.update_validator_name(preset.name(), ctx);
        system.update_validator_description(preset.description(), ctx);
        system.update_validator_image_url(preset.image_url(), ctx);
        system.update_validator_project_url(preset.project_url(), ctx);
    });

    runner.add_validator();
    runner.advance_epoch(option::none()).destroy_zero();
    runner.finish();
}

#[test, expected_failure(abort_code = validator_set::ENotValidatorCandidate)]
fun cannot_update_net_address_of_non_candidate_validator() {
    let mut runner = test_runner::new().validators_count(1).build();
    let preset = validator_preset::preset(2);
    let candidate = validator_builder::preset(1).initial_stake(10000).build(runner.ctx());
    let candidate_addr = candidate.sui_address();

    runner.set_sender(candidate_addr).add_validator_candidate(candidate);
    runner.add_validator();
    runner.system_tx!(|system, ctx| {
        system.update_candidate_validator_network_address(preset.net_address(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::ENotValidatorCandidate)]
fun cannot_update_p2p_address_of_non_candidate_validator() {
    let mut runner = test_runner::new().validators_count(1).build();
    let preset = validator_preset::preset(2);
    let candidate = validator_builder::preset(1).initial_stake(10000).build(runner.ctx());
    let candidate_addr = candidate.sui_address();

    runner.set_sender(candidate_addr).add_validator_candidate(candidate);
    runner.add_validator();
    runner.system_tx!(|system, ctx| {
        system.update_candidate_validator_p2p_address(preset.p2p_address(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::ENotValidatorCandidate)]
fun cannot_update_primary_address_of_non_candidate_validator() {
    let mut runner = test_runner::new().validators_count(1).build();
    let preset = validator_preset::preset(2);
    let candidate = validator_builder::preset(1).initial_stake(10000).build(runner.ctx());
    let candidate_addr = candidate.sui_address();

    runner.set_sender(candidate_addr).add_validator_candidate(candidate);
    runner.add_validator();
    runner.system_tx!(|system, ctx| {
        system.update_candidate_validator_primary_address(preset.primary_address(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::ENotValidatorCandidate)]
fun cannot_update_worker_address_of_non_candidate_validator() {
    let mut runner = test_runner::new().validators_count(1).build();
    let preset = validator_preset::preset(2);
    let candidate = validator_builder::preset(1).initial_stake(10000).build(runner.ctx());
    let candidate_addr = candidate.sui_address();

    runner.set_sender(candidate_addr).add_validator_candidate(candidate);
    runner.add_validator();
    runner.system_tx!(|system, ctx| {
        system.update_candidate_validator_worker_address(preset.worker_address(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::ENotValidatorCandidate)]
fun cannot_update_network_pubkey_bytes_of_non_candidate_validator() {
    let mut runner = test_runner::new().validators_count(1).build();
    let preset = validator_preset::preset(2);
    let candidate = validator_builder::preset(1).initial_stake(10000).build(runner.ctx());
    let candidate_addr = candidate.sui_address();

    runner.set_sender(candidate_addr).add_validator_candidate(candidate);
    runner.add_validator();
    runner.system_tx!(|system, ctx| {
        system.update_candidate_validator_network_pubkey(preset.network_pubkey_bytes(), ctx);
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::ENotValidatorCandidate)]
fun cannot_update_worker_pubkey_bytes_of_non_candidate_validator() {
    let mut runner = test_runner::new().validators_count(1).build();
    let preset = validator_preset::preset(2);
    let candidate = validator_builder::preset(1).initial_stake(10000).build(runner.ctx());
    let candidate_addr = candidate.sui_address();

    runner.set_sender(candidate_addr).add_validator_candidate(candidate);
    runner.add_validator();
    runner.system_tx!(|system, ctx| {
        system.update_candidate_validator_worker_pubkey(preset.worker_pubkey_bytes(), ctx);
    });

    abort
}

#[test]
fun active_validator_update_metadata() {
    let validator_addr = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
    // pubkey generated with protocol key on seed [0; 32]
    let pubkey =
        x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
    // pop generated using the protocol key and address with [fn test_proof_of_possession]
    let pop =
        x"b01cc86f421beca7ab4cfca87c0799c4d038c199dd399fbec1924d4d4367866dba9e84d514710b91feb65316e4ceef43";

    // pubkey generated with protocol key on seed [1; 32]
    let pubkey1 =
        x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c";
    let pop1 =
        x"a8a0bcaf04e13565914eb22fa9f27a76f297db04446860ee2b923d10224cedb130b30783fb60b12556e7fc50e5b57a86";

    let new_validator_addr = @0x8e3446145b0c7768839d71840df389ffa3b9742d0baaff326a3d453b595f87d7;
    // pubkey generated with protocol key on seed [2; 32]
    let new_pubkey =
        x"adf2e2350fe9a58f3fa50777499f20331c4550ab70f6a4fb25a58c61b50b5366107b5c06332e71bb47aa99ce2d5c07fe0dab04b8af71589f0f292c50382eba6ad4c90acb010ab9db7412988b2aba1018aaf840b1390a8b2bee3fde35b4ab7fdf";
    let new_pop =
        x"926fdb08b2b46d802e3642044f215dcb049e6c17a376a272ffd7dba32739bb995370966698ab235ee172fbd974985cfe";

    // pubkey generated with protocol key on seed [3; 32]
    let new_pubkey1 =
        x"91b8de031e0b60861c655c8168596d98b065d57f26f287f8c810590b06a636eff13c4055983e95b2f60a4d6ba5484fa4176923d1f7807cc0b222ddf6179c1db099dba0433f098aae82542b3fd27b411d64a0a35aad01b2c07ac67f7d0a1d2c11";
    let new_pop1 =
        x"b61913eb4dc7ea1d92f174e1a3c6cad3f49ae8de40b13b69046ce072d8d778bfe87e734349c7394fd1543fff0cb6e2d0";

    let validator = validator_builder::preset(0)
        .sui_address(validator_addr)
        .protocol_pubkey_bytes(pubkey)
        // prettier-ignore
        .network_pubkey_bytes(vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62])
        // prettier-ignore
        .worker_pubkey_bytes(vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25])
        .proof_of_possession(pop)
        .name(b"ValidatorName")
        .description(b"description")
        .image_url(b"image_url")
        .project_url(b"project_url")
        .net_address(b"/ip4/127.0.0.1/tcp/80")
        .initial_stake(100);

    let mut runner = test_runner::new()
        .validators(vector[validator])
        .storage_fund_amount(0)
        .sui_supply_amount(1000)
        .build();

    // ===

    runner.set_sender(validator_addr).scenario_fn!(|scenario| {
        let mut system_state = scenario.take_shared<SuiSystemState>();
        // prettier-ignore
        update_metadata(
            scenario,
            &mut system_state,
            b"validator_new_name",
            pubkey1,
            pop1,
            b"/ip4/42.42.42.42/tcp/80",
            b"/ip4/43.43.43.43/udp/80",
            b"/ip4/168.168.168.168/udp/80",
            b"/ip4/168.168.168.168/udp/81",
            vector[148, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
            vector[215, 64, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
        );

        let validator = system_state.active_validator_by_address(validator_addr);

        // prettier-ignore
        verify_metadata(
            validator,
            b"validator_new_name",
            pubkey,
            pop,
            b"/ip4/127.0.0.1/udp/80",
            b"/ip4/127.0.0.1/udp/80",
            b"/ip4/127.0.0.1/tcp/80",
            b"/ip4/127.0.0.1/udp/80",
            vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
            vector[68, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
            pubkey1,
            pop1,
            b"/ip4/42.42.42.42/tcp/80",
            b"/ip4/43.43.43.43/udp/80",
            b"/ip4/168.168.168.168/udp/80",
            b"/ip4/168.168.168.168/udp/81",
            vector[148, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
            vector[215, 64, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
        );

        test_scenario::return_shared(system_state);
    });

    // === Test pending validator metadata changes ===

    runner.set_sender(new_validator_addr); // required for `.build()` to work

    let validator = validator_builder::preset(0)
        .sui_address(new_validator_addr)
        .protocol_pubkey_bytes(new_pubkey)
        // prettier-ignore
        .network_pubkey_bytes(vector[33, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62])
        // prettier-ignore
        .worker_pubkey_bytes(vector[69, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25])
        .proof_of_possession(new_pop)
        .name(b"ValidatorName2")
        .description(b"description2")
        .image_url(b"image_url2")
        .project_url(b"project_url2")
        .net_address(b"/ip4/127.0.0.2/tcp/80")
        .p2p_address(b"/ip4/127.0.0.2/udp/80")
        .primary_address(b"/ip4/192.192.192.192/udp/80")
        .worker_address(b"/ip4/192.192.192.192/udp/81")
        .gas_price(1)
        .commission_rate(0)
        .is_active_at_genesis(false)
        .build(runner.ctx());

    runner.set_sender(new_validator_addr).add_validator_candidate(validator);
    runner.set_sender(new_validator_addr).stake_with(new_validator_addr, 100);
    runner.set_sender(new_validator_addr).add_validator();
    runner.advance_epoch(option::none()).destroy_for_testing();

    runner.scenario_fn!(|scenario| {
        let mut system_state = scenario.take_shared<SuiSystemState>();
        // prettier-ignore
        update_metadata(
            scenario,
            &mut system_state,
            b"new_validator_new_name",
            new_pubkey1,
            new_pop1,
            b"/ip4/66.66.66.66/tcp/80",
            b"/ip4/77.77.77.77/udp/80",
            b"/ip4/88.88.88.88/udp/80",
            b"/ip4/88.88.88.88/udp/81",
            vector[215, 65, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
            vector[149, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
        );

        let validator = system_state.active_validator_by_address(new_validator_addr);

        // prettier-ignore
        verify_metadata(
            validator,
            b"new_validator_new_name",
            new_pubkey,
            new_pop,
            b"/ip4/192.192.192.192/udp/80",
            b"/ip4/192.192.192.192/udp/81",
            b"/ip4/127.0.0.2/tcp/80",
            b"/ip4/127.0.0.2/udp/80",
            vector[33, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
            vector[69, 55, 206, 25, 199, 14, 169, 53, 68, 92, 142, 136, 174, 149, 54, 215, 101, 63, 249, 206, 197, 98, 233, 80, 60, 12, 183, 32, 216, 88, 103, 25],
            new_pubkey1,
            new_pop1,
            b"/ip4/66.66.66.66/tcp/80",
            b"/ip4/77.77.77.77/udp/80",
            b"/ip4/88.88.88.88/udp/80",
            b"/ip4/88.88.88.88/udp/81",
            vector[215, 65, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
            vector[149, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
        );

        test_scenario::return_shared(system_state);
    });

    // === Advance epoch to effectuate the metadata changes ===

    let opts = runner.advance_epoch_opts().protocol_version(65).epoch_start_time(99_9999_999);
    runner.advance_epoch(option::some(opts)).destroy_for_testing();

    runner.system_tx!(|system, _| {
        let validator = system.active_validator_by_address(validator_addr);
        // prettier-ignore
        verify_metadata_after_advancing_epoch(
            validator,
            b"validator_new_name",
            pubkey1,
            pop1,
            b"/ip4/168.168.168.168/udp/80",
            b"/ip4/168.168.168.168/udp/81",
            b"/ip4/42.42.42.42/tcp/80",
            b"/ip4/43.43.43.43/udp/80",
            vector[148, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
            vector[215, 64, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
        );

        let validator = system.active_validator_by_address(new_validator_addr);
        // prettier-ignore
        verify_metadata_after_advancing_epoch(
            validator,
            b"new_validator_new_name",
            new_pubkey1,
            new_pop1,
            b"/ip4/88.88.88.88/udp/80",
            b"/ip4/88.88.88.88/udp/81",
            b"/ip4/66.66.66.66/tcp/80",
            b"/ip4/77.77.77.77/udp/80",
            vector[215, 65, 85, 185, 231, 116, 69, 151, 97, 79, 4, 183, 20, 70, 84, 51, 211, 162, 115, 221, 73, 241, 240, 171, 192, 25, 232, 106, 175, 162, 176, 43],
            vector[149, 117, 212, 171, 44, 104, 167, 11, 177, 100, 4, 55, 17, 235, 117, 45, 117, 84, 159, 49, 14, 159, 239, 246, 237, 21, 83, 166, 112, 53, 62, 199],
        );
    });

    runner.finish();
}

#[test]
// Scenario:
// 1. Submit a candidate validator
// 2. Request removal of the candidate
// 3. Submit the same candidate validator again
// 4. Get validator into the active set
// 5. Request removal of the validator
// 6. Submit the same validator again
fun duplicate_metadata_resubmission_after_inactive() {
    let mut runner = test_runner::new()
        .validators_initial_stake(1_000_000)
        .validators_count(3)
        .build();

    // The first 3 validators are using presets 0-2.
    let preset = validator_preset::preset(3);
    let validator = validator_builder::from_preset(preset).build(runner.ctx());
    let validator_address = validator.sui_address();

    // Submit a candidate validator, then remove it
    runner.add_validator_candidate(validator);
    runner.set_sender(validator_address).remove_validator_candidate();

    // Create a new candidate with the same metadata
    let validator = validator_builder::from_preset(preset).build(runner.ctx());
    runner.add_validator_candidate(validator);

    // Stake with the validator, so they have enough stake
    runner.stake_with(validator_address, 1_000_000);
    runner.set_sender(validator_address).add_validator();

    // Advance epoch to effectuate the validator addition
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Request removal of the validator
    runner.set_sender(validator_address).remove_validator();

    // Wait another epoch to effectuate the removal
    runner.advance_epoch(option::none()).destroy_for_testing();

    // Create a new candidate with the same metadata again
    let validator = validator_builder::from_preset(preset).build(runner.ctx());
    runner.add_validator_candidate(validator);

    runner.finish();
}

#[test, expected_failure(abort_code = validator::EMetadataInvalidWorkerPubkey)]
fun add_validator_candidate_failure_invalid_metadata() {
    let mut runner = test_runner::new().validators_count(3).build();

    // Generated using [fn test_proof_of_possession]
    let new_validator_addr = @0x8e3446145b0c7768839d71840df389ffa3b9742d0baaff326a3d453b595f87d7;
    // prettier-ignore
    let pubkey = x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
    // prettier-ignore
    let pop = x"83809369ce6572be211512d85621a075ee6a8da57fbb2d867d05e6a395e71f10e4e957796944d68a051381eb91720fba";

    runner.set_sender(new_validator_addr).system_tx!(|system, ctx| {
        system.request_add_validator_candidate(
            pubkey,
            // prettier-ignore
            vector[32, 219, 38, 23, 242, 109, 116, 235, 225, 192, 219, 45, 40, 124, 162, 25, 33, 68, 52, 41, 123, 9, 98, 11, 184, 150, 214, 62, 60, 210, 121, 62],
            vector[42], // invalid
            pop,
            b"ValidatorName2",
            b"description2",
            b"image_url2",
            b"project_url2",
            b"/ip4/127.0.0.2/tcp/80",
            b"/ip4/127.0.0.2/udp/80",
            b"/ip4/127.0.0.1/udp/80",
            b"/ip4/127.0.0.1/udp/80",
            1,
            0,
            ctx,
        );
    });

    abort
}

#[test, expected_failure(abort_code = validator_set::EAlreadyValidatorCandidate)]
fun add_validator_candidate_failure_double_register() {
    let mut runner = test_runner::new().validators_count(3).build();

    // The first 3 validators are using presets 0-2.
    let preset = validator_preset::preset(3);
    let validator = validator_builder::from_preset(preset).build(runner.ctx());
    runner.add_validator_candidate(validator);

    // Try repeat the same procedure and fail.
    let validator = validator_builder::from_preset(preset).build(runner.ctx());
    runner.add_validator_candidate(validator);

    abort
}

#[test, expected_failure(abort_code = validator_set::EDuplicateValidator)]
fun add_validator_candidate_failure_duplicate_with_active() {
    let validator_addr = @0xaf76afe6f866d8426d2be85d6ef0b11f871a251d043b2f11e15563bf418f5a5a;
    // Seed [0; 32]
    // prettier-ignore
    let pubkey = x"99f25ef61f8032b914636460982c5cc6f134ef1ddae76657f2cbfec1ebfc8d097374080df6fcf0dcb8bc4b0d8e0af5d80ebbff2b4c599f54f42d6312dfc314276078c1cc347ebbbec5198be258513f386b930d02c2749a803e2330955ebd1a10";
    // prettier-ignore
    let pop = x"b01cc86f421beca7ab4cfca87c0799c4d038c199dd399fbec1924d4d4367866dba9e84d514710b91feb65316e4ceef43";

    // new validator
    let new_addr = @0x1a4623343cd42be47d67314fce0ad042f3c82685544bc91d8c11d24e74ba7357;
    // Seed [1; 32]
    // prettier-ignore
    let new_pubkey = x"96d19c53f1bee2158c3fcfb5bb2f06d3a8237667529d2d8f0fbb22fe5c3b3e64748420b4103674490476d98530d063271222d2a59b0f7932909cc455a30f00c69380e6885375e94243f7468e9563aad29330aca7ab431927540e9508888f0e1c";
    // prettier-ignore
    let new_pop = x"932336c35a8c393019c63eb0f7d385dd4e0bd131f04b54cf45aa9544f14dca4dab53bd70ffcb8e0b34656e4388309720";

    let initial_validator = validator_builder::preset(0)
        .sui_address(validator_addr)
        .protocol_pubkey_bytes(pubkey)
        .proof_of_possession(pop);

    let mut runner = test_runner::new()
        .validators(vector[initial_validator])
        .sui_supply_amount(1000)
        .build();

    // address, public key and pop are new and valid, yet the validator metadata is duplicate
    let new_validator = validator_builder::preset(0)
        .sui_address(new_addr)
        .network_pubkey_bytes(new_pubkey)
        .proof_of_possession(new_pop)
        .build(runner.ctx());

    runner.add_validator_candidate(new_validator);

    abort
}

// Note: `pop` MUST be a valid signature using sui_address and protocol_pubkey_bytes.
// To produce a valid PoP, run [fn test_proof_of_possession].
fun update_metadata(
    scenario: &mut Scenario,
    system_state: &mut SuiSystemState,
    name: vector<u8>,
    protocol_pub_key: vector<u8>,
    pop: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    primary_address: vector<u8>,
    worker_address: vector<u8>,
    network_pubkey: vector<u8>,
    worker_pubkey: vector<u8>,
) {
    let ctx = scenario.ctx();
    system_state.update_validator_name(name, ctx);
    system_state.update_validator_description(b"new_desc", ctx);
    system_state.update_validator_image_url(b"new_image_url", ctx);
    system_state.update_validator_project_url(b"new_project_url", ctx);
    system_state.update_validator_next_epoch_network_address(network_address, ctx);
    system_state.update_validator_next_epoch_p2p_address(p2p_address, ctx);
    system_state.update_validator_next_epoch_primary_address(primary_address, ctx);
    system_state.update_validator_next_epoch_worker_address(worker_address, ctx);
    system_state.update_validator_next_epoch_protocol_pubkey(
        protocol_pub_key,
        pop,
        ctx,
    );
    system_state.update_validator_next_epoch_network_pubkey(network_pubkey, ctx);
    system_state.update_validator_next_epoch_worker_pubkey(worker_pubkey, ctx);
}

fun verify_metadata(
    validator: &Validator,
    name: vector<u8>,
    protocol_pub_key: vector<u8>,
    pop: vector<u8>,
    current_primary_address: vector<u8>,
    current_worker_address: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    network_pubkey: vector<u8>,
    worker_pubkey: vector<u8>,
    new_protocol_pub_key: vector<u8>,
    new_pop: vector<u8>,
    new_network_address: vector<u8>,
    new_p2p_address: vector<u8>,
    new_primary_address: vector<u8>,
    new_worker_address: vector<u8>,
    new_network_pubkey: vector<u8>,
    new_worker_pubkey: vector<u8>,
) {
    // Current epoch
    verify_current_epoch_metadata(
        validator,
        name,
        protocol_pub_key,
        pop,
        current_primary_address,
        current_worker_address,
        network_address,
        p2p_address,
        network_pubkey,
        worker_pubkey,
    );

    // Next epoch
    assert!(
        validator.next_epoch_network_address() == &option::some(new_network_address.to_string()),
    );
    assert!(validator.next_epoch_p2p_address() == &option::some(new_p2p_address.to_string()));
    assert!(
        validator.next_epoch_primary_address() == &option::some(new_primary_address.to_string()),
    );
    assert!(validator.next_epoch_worker_address() == &option::some(new_worker_address.to_string()));
    assert!(validator.next_epoch_protocol_pubkey_bytes() == &option::some(new_protocol_pub_key), 0);
    assert!(validator.next_epoch_proof_of_possession() == &option::some(new_pop), 0);
    assert!(validator.next_epoch_worker_pubkey_bytes() == &option::some(new_worker_pubkey), 0);
    assert!(validator.next_epoch_network_pubkey_bytes() == &option::some(new_network_pubkey), 0);
}

fun verify_current_epoch_metadata(
    validator: &Validator,
    name: vector<u8>,
    protocol_pub_key: vector<u8>,
    pop: vector<u8>,
    primary_address: vector<u8>,
    worker_address: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    network_pubkey_bytes: vector<u8>,
    worker_pubkey_bytes: vector<u8>,
) {
    // Current epoch
    assert!(validator.name() == &name.to_string());
    assert!(validator.description() == &b"new_desc".to_string());
    assert!(validator.image_url() == &url::new_unsafe_from_bytes(b"new_image_url"));
    assert!(validator.project_url() == &url::new_unsafe_from_bytes(b"new_project_url"));
    assert!(validator.network_address() == &network_address.to_string());
    assert!(validator.p2p_address() == &p2p_address.to_string());
    assert!(validator.primary_address() == &primary_address.to_string());
    assert!(validator.worker_address() == &worker_address.to_string());
    assert!(validator.protocol_pubkey_bytes() == &protocol_pub_key);
    assert!(validator.proof_of_possession() == &pop);
    assert!(validator.worker_pubkey_bytes() == &worker_pubkey_bytes);
    assert!(validator.network_pubkey_bytes() == &network_pubkey_bytes);
}

fun verify_metadata_after_advancing_epoch(
    validator: &Validator,
    name: vector<u8>,
    protocol_pub_key: vector<u8>,
    pop: vector<u8>,
    primary_address: vector<u8>,
    worker_address: vector<u8>,
    network_address: vector<u8>,
    p2p_address: vector<u8>,
    network_pubkey: vector<u8>,
    worker_pubkey: vector<u8>,
) {
    // Current epoch
    verify_current_epoch_metadata(
        validator,
        name,
        protocol_pub_key,
        pop,
        primary_address,
        worker_address,
        network_address,
        p2p_address,
        network_pubkey,
        worker_pubkey,
    );

    // Next epoch
    assert!(validator.next_epoch_network_address().is_none());
    assert!(validator.next_epoch_p2p_address().is_none());
    assert!(validator.next_epoch_primary_address().is_none());
    assert!(validator.next_epoch_worker_address().is_none());
    assert!(validator.next_epoch_protocol_pubkey_bytes().is_none());
    assert!(validator.next_epoch_proof_of_possession().is_none());
    assert!(validator.next_epoch_worker_pubkey_bytes().is_none());
    assert!(validator.next_epoch_network_pubkey_bytes().is_none());
}
