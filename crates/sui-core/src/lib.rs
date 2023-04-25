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
pub mod consensus_validator;
pub mod db_checkpoint_handler;
pub mod epoch;
pub mod event_handler;
mod execution_driver;
mod math;
pub mod metrics;
pub mod module_cache_metrics;
pub mod narwhal_manager;
pub mod quorum_driver;
pub mod safe_client;
mod scoring_decision;
mod stake_aggregator;
pub mod state_accumulator;
pub mod storage;
pub mod streamer;
#[cfg(feature = "test-utils")]
pub mod test_utils;
pub mod transaction_input_checker;
mod transaction_manager;
pub mod transaction_orchestrator;

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
pub mod test_authority_clients;
#[cfg(test)]
#[path = "unit_tests/type_param_tests.rs"]
mod type_param_tests;

pub mod signature_verifier;

pub mod runtime;
mod transaction_signing_filter;

pub const SUI_CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
