// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::runtime_value::MoveValue;
use rand::{seq::SliceRandom, thread_rng, Rng};
use regex::Regex;
use std::sync::Arc;
use std::time::Duration;
use sui_config::node::AuthorityOverloadConfig;
use sui_macros::sim_test;
use sui_surfer::surf_strategy::{ErrorChecks, ExitCondition, RewriteResponse, SurfStrategy};
use sui_surfer::surfer_state::{get_type_tag, EntryFunction, SurferState};
use sui_swarm_config::genesis_config::GenesisConfig;
use sui_types::transaction::CallArg;
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use test_cluster::TestClusterBuilder;
use tracing::{error, info};

#[sim_test]
async fn test_sui_surfer_framework() {
    let test_cluster: Arc<_> = TestClusterBuilder::new()
        // extra coins so that surfer can create many stake objects
        .set_genesis_config(GenesisConfig::custom_genesis(2, 200, 100 * 1_000_000_000))
        // TODO: set to 1 validator when that is supported
        .with_num_validators(2)
        .with_epoch_duration_ms(20000)
        .with_authority_overload_config(AuthorityOverloadConfig {
            // Disable system overload checks for the test - during tests with crashes,
            // it is possible for overload protection to trigger due to validators
            // having queued certs which are missing dependencies.
            check_system_overload_at_execution: false,
            check_system_overload_at_signing: false,
            ..Default::default()
        })
        .with_submit_delay_step_override_millis(3000)
        .build()
        .await
        .into();

    // First, do a short surf with only request_add_stake in order to pre-populate a
    // bunch of staked objects
    let mut surf_strategy = SurfStrategy::new(Duration::from_millis(100));
    surf_strategy.set_exit_condition(ExitCondition::Timeout(Duration::from_secs(300)));

    let filter_fn = move |module: &str, func: &str| {
        module == "sui_system" && func == "request_add_stake_non_entry"
    };

    surf_strategy.add_call_rewriter(
        &["sui_system::request_add_stake_non_entry"],
        Arc::new(
            |state: &SurferState, entry_fn: &EntryFunction, args: &mut Vec<CallArg>| {
                let coin_type_tag = get_type_tag(entry_fn.return_type.clone().unwrap()).unwrap();
                if state.matching_owned_objects_count(&coin_type_tag) >= 30 {
                    // stop calling this function once we have enough staked objects
                    RewriteResponse::ForgetEntryPoint
                } else {
                    // only use the first validator
                    let validator_addresses = state.cluster.get_validator_addresses();
                    let validator = validator_addresses[0];
                    args[2] = CallArg::Pure(
                        bcs::to_bytes(&MoveValue::Address((validator).into())).unwrap(),
                    );
                    RewriteResponse::Call
                }
            },
        ),
    );

    tokio::time::timeout(
        // Surfer will exit as soon as it runs out of entry points to call, so we should
        // not hit this timeout
        Duration::from_secs(300),
        sui_surfer::run_with_test_cluster_and_strategy(
            surf_strategy,
            vec![SUI_SYSTEM_PACKAGE_ID.into()],
            Some(Arc::new(filter_fn)),
            test_cluster.clone(),
            1, // skip first account for use by bench_task
        ),
    )
    .await
    .unwrap();

    // now surf the whole contract.

    let mut surf_strategy = SurfStrategy::new(Duration::from_millis(100));
    surf_strategy.set_exit_condition(ExitCondition::AllEntryPointsCalledSuccessfully);
    surf_strategy.set_error_checking_mode(ErrorChecks::WellFormed);

    // must split at least 1 SUI out of stake object
    surf_strategy.add_call_rewriter(
        &["staking_pool::split", "staking_pool::split_staked_sui"],
        Arc::new(
            |_state: &SurferState, _entry_fn: &EntryFunction, args: &mut Vec<CallArg>| {
                let mut rng = rand::thread_rng();
                // with 50% chance, choose a small amount, otherwise choose a random
                // amount (important to test guard against splitting large amount).
                let amount: u64 = if rng.gen_bool(0.5) {
                    1_000_000_000 + rng.gen_range(0..1_000_000)
                } else {
                    rng.gen()
                };
                args[1] = CallArg::Pure(bcs::to_bytes(&amount).unwrap());
                RewriteResponse::Call
            },
        ),
    );

    // fungible stake has no minimum split amount, but it must be a small number in order
    // not to underflow
    surf_strategy.add_call_rewriter(
        &["staking_pool::split_fungible_staked_sui"],
        Arc::new(
            |_state: &SurferState, _entry_fn: &EntryFunction, args: &mut Vec<CallArg>| {
                let mut rng = rand::thread_rng();
                // with 50% chance, choose a small amount, otherwise choose a random
                // amount (important to test guard against splitting large amount).
                let amount: u64 = if rng.gen_bool(0.5) {
                    rng.gen_range(0..1_000_000)
                } else {
                    rng.gen()
                };
                args[1] = CallArg::Pure(bcs::to_bytes(&amount).unwrap());
                RewriteResponse::Call
            },
        ),
    );

    surf_strategy.add_call_rewriter(
        &[
            "sui_system::request_add_stake",
            "sui_system::request_add_stake_non_entry",
        ],
        Arc::new(
            |state: &SurferState, _entry_fn: &EntryFunction, args: &mut Vec<CallArg>| {
                let validator_addresses = state.cluster.get_validator_addresses();

                let join_staked_sui_successful = state
                    .stats
                    .borrow()
                    .unique_move_functions_called_success
                    .iter()
                    .any(|(_, module, func)| module == "staking_pool" && func == "join_staked_sui");

                let validator = if join_staked_sui_successful {
                    // once we've had at least once successful join_staked_sui call, pick
                    // random validators
                    validator_addresses.choose(&mut thread_rng()).unwrap()
                } else {
                    // before there is a successful join_staked_sui call, use the same validator
                    // so that all of the staked sui objects have the same pool.
                    &validator_addresses[0]
                };

                let validator = MoveValue::Address((*validator).into());

                args[2] = CallArg::Pure(bcs::to_bytes(&validator).unwrap());
                RewriteResponse::Call
            },
        ),
    );

    // TODO:
    // - add support for more of the validator methods
    // - support methods that return types without `key` - these can't be transferred to the sender
    // - request_add_stake_mul_coin needs a rewriter that picks N coins and puts them in a vector
    let exclude_regex = "(add|remove|update).*_validator\
                        |(^validator::)\
                        |(^staking_pool::(fungible_staked_sui_pool_id|pool_id)$)\
                        |staking_pool::pool_token_exchange_rate_at_epoch\
                        |::report_validator\
                        |::request_add_stake_mul_coin\
                        |::request_set_commission_rate\
                        |::request_set_gas_price\
                        |::rotate_operation_cap\
                        |::set_candidate_validator_commission_rate\
                        |::set_candidate_validator_gas_price\
                        |::undo_report_validator\
                        |::validator_staking_pool_id";

    let exclude_regex = Regex::new(exclude_regex).unwrap();
    let filter_fn =
        move |module: &str, func: &str| !exclude_regex.is_match(&format!("{}::{}", module, func));

    let results = tokio::time::timeout(
        Duration::from_secs(300),
        sui_surfer::run_with_test_cluster_and_strategy(
            surf_strategy,
            vec![SUI_SYSTEM_PACKAGE_ID.into()],
            Some(Arc::new(filter_fn)),
            test_cluster.clone(),
            0,
        ),
    )
    .await;

    match results {
        Ok(results) => {
            info!("sui_surfer test complete with results: {results:?}");
            assert!(results.num_successful_transactions > 0);
            assert!(!results.unique_move_functions_called.is_empty());
        }
        Err(_) => {
            // Sometimes we are not able to call join_staked_sui successfully because it
            // the surfer doesn't find compatible staked_sui objects to join.
            // TODO: fix that with a rewriter that finds compatible objects, and make this
            // a fatal error.
            error!("sui_surfer test timed out");
        }
    }

    test_cluster.fullnode_handle.sui_node.with(|node| {
        assert!(node.state().current_epoch_for_testing() > 3);
    });
}
