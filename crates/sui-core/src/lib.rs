// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

extern crate core;

pub mod authority;
pub mod authority_aggregator;
pub mod authority_client;
pub mod authority_server;
pub mod checkpoints;
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
pub mod metrics;
#[cfg(any(test, feature = "test-utils"))]
pub mod mock_consensus;
pub mod module_cache_metrics;
pub mod mysticeti_adapter;
pub mod overload_monitor;
pub(crate) mod post_consensus_tx_reorder;
pub mod quorum_driver;
pub mod rest_index;
pub mod safe_client;
mod scoring_decision;
mod stake_aggregator;
pub mod state_accumulator;
pub mod storage;
pub mod streamer;
pub mod subscription_handler;
#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;
pub mod traffic_controller;
mod transaction_input_loader;
mod transaction_manager;
pub mod transaction_orchestrator;
mod transaction_outputs;
pub mod validator_tx_finalizer;
pub mod verify_indexes;

#[cfg(test)]
#[path = "unit_tests/congestion_control_tests.rs"]
mod congestion_control_tests;
#[cfg(test)]
#[path = "unit_tests/move_package_management_tests.rs"]
mod move_package_management_tests;
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
#[cfg(any(test, feature = "test-utils"))]
pub mod test_authority_clients;
#[cfg(test)]
#[path = "unit_tests/transfer_to_object_tests.rs"]
mod transfer_to_object_tests;
#[cfg(test)]
#[path = "unit_tests/type_param_tests.rs"]
mod type_param_tests;

pub mod signature_verifier;

pub mod runtime;
mod transaction_signing_filter;
