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
pub mod epoch;
pub mod event_handler;
mod execution_driver;
pub mod metrics;
mod module_cache_gauge;
pub mod narwhal_manager;
mod notify_once;
pub mod quorum_driver;
pub mod safe_client;
mod stake_aggregator;
pub mod state_accumulator;
pub mod storage;
pub mod streamer;
pub mod tbls;
pub mod test_utils;
pub mod transaction_input_checker;
mod transaction_manager;
pub mod transaction_orchestrator;
pub mod validator_info;

#[cfg(test)]
#[path = "unit_tests/pay_sui_tests.rs"]
mod pay_sui_tests;
pub mod test_authority_clients;

pub const SUI_CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
