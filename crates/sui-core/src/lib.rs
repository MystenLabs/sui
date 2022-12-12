// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
pub mod authority;
pub mod authority_active;
pub mod authority_aggregator;
pub mod authority_batch;
pub mod authority_client;
pub mod authority_server;
pub mod checkpoints;
pub mod consensus_adapter;
pub mod consensus_validator;
pub mod epoch;
pub mod event_handler;
pub mod execution_engine;
pub mod metrics;
pub mod quorum_driver;
pub mod safe_client;
pub mod storage;
pub mod streamer;
pub mod tbls;
pub mod test_utils;
pub mod transaction_input_checker;
pub mod transaction_orchestrator;
pub mod transaction_streamer;
pub mod validator_info;

mod consensus_handler;
mod execution_driver;
mod histogram;
mod module_cache_gauge;
mod node_sync;
mod notify_once;
mod query_helpers;
mod stake_aggregator;
mod transaction_manager;

pub const SUI_CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
