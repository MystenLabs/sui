// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The simulator tests build into a few binaries instead of one per file, because each test
// binary links the whole dependency graph. They are split so that they compile in parallel.
#![cfg(msim)]

mod address_balance_compatibility_tests;
mod alias_tests;
mod allowance_tests;
mod authenticated_events_api_tests;
mod authenticated_events_end_to_end_test;
mod checkpoint_tests;
mod derived_object_release_tests;
mod discovery_tests;
mod ecdsa_r1_ecrecover_tests;
mod effects_size_limit_tests;
mod execution_time_chunking_tests;
mod forwarding_address_tests;
mod full_node_tests;
mod grpc_simulate_gas_coin_tests;
mod grpc_simulate_gasless_tests;
mod implicit_clock_tests;
mod mldsa_tests;
mod object_balance_tests;
mod observer_checkpoint_tests;
mod party_objects_tests;
mod protocol_version_tests;
mod shared_objects_version_tests;
mod traffic_control_tests;
mod transaction_orchestrator_tests;
