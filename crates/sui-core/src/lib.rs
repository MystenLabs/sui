// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate core;

pub mod accumulators;
pub mod authority;
pub mod authority_aggregator;
pub mod authority_client;
pub mod authority_server;
pub mod checkpoints;
pub mod congestion_tracker;
pub mod consensus_adapter;
pub mod consensus_handler;
pub mod consensus_manager;
pub mod consensus_throughput_calculator;
pub(crate) mod consensus_types;
pub mod consensus_validator;
pub mod db_checkpoint_handler;
pub mod epoch;
pub mod execution_cache;
mod execution_driver;
pub mod execution_scheduler;
mod fallback_fetch;
pub mod global_state_hasher;
pub mod jsonrpc_index;
pub mod metrics;
pub mod mock_checkpoint_builder;
pub mod mock_consensus;
pub mod module_cache_metrics;
pub mod mysticeti_adapter;
pub mod overload_monitor;
mod par_index_live_object_set;
pub(crate) mod post_consensus_tx_reorder;
pub mod rpc_index;
pub mod runtime;
pub mod safe_client;
mod scoring_decision;
pub mod signature_verifier;
mod stake_aggregator;
mod status_aggregator;
pub mod storage;
pub mod streamer;
pub mod subscription_handler;
pub mod test_utils;
pub mod traffic_controller;
pub mod transaction_driver;
mod transaction_input_loader;
pub mod transaction_orchestrator;
mod transaction_outputs;
mod transaction_signing_filter;
pub mod validator_client_monitor;
pub mod verify_indexes;

#[cfg(test)]
#[path = "unit_tests/congestion_control_tests.rs"]
mod congestion_control_tests;
#[path = "unit_tests/consensus_test_utils.rs"]
pub mod consensus_test_utils;
#[cfg(test)]
#[path = "unit_tests/move_package_publish_tests.rs"]
mod move_package_publish_tests;
#[cfg(test)]
#[path = "unit_tests/move_package_tests.rs"]
mod move_package_tests;
#[cfg(test)]
#[path = "unit_tests/move_package_upgrade_tests.rs"]
mod move_package_upgrade_tests;
#[cfg(test)]
#[path = "unit_tests/pay_sui_tests.rs"]
mod pay_sui_tests;
#[cfg(test)]
#[path = "unit_tests/shared_object_deletion_tests.rs"]
mod shared_object_deletion_tests;
#[cfg(test)]
pub mod test_authority_clients;
#[cfg(test)]
#[path = "unit_tests/transfer_to_object_tests.rs"]
mod transfer_to_object_tests;
#[cfg(test)]
#[path = "unit_tests/type_param_tests.rs"]
mod type_param_tests;
#[cfg(test)]
#[path = "unit_tests/unit_test_utils.rs"]
mod unit_test_utils;
