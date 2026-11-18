// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// All integration tests build into this one binary. Each separate test binary links the full
// dependency graph, which made linking dominate CI build time.

mod address_balance_compatibility_tests;
mod address_balance_tests;
mod admission_queue_tests;
mod alias_tests;
mod allowance_tests;
mod authenticated_events_api_tests;
mod authenticated_events_client_test;
mod authenticated_events_end_to_end_test;
mod checkpoint_rate_tests;
mod checkpoint_tests;
mod db_shell_tests;
mod derived_object_release_tests;
mod discovery_tests;
mod dynamic_committee_tests;
mod ecdsa_r1_ecrecover_tests;
mod effects_size_limit_tests;
mod execution_time_chunking_tests;
mod forwarding_address_tests;
mod full_node_tests;
mod gasless_tests;
mod get_object_inclusion_proof_tests;
mod grpc_simulate_gas_coin_tests;
mod grpc_simulate_gasless_tests;
mod implicit_clock_tests;
mod mldsa_tests;
mod multisig_tests;
mod object_balance_tests;
mod object_deletion_tests;
mod observer_checkpoint_tests;
mod onsite_reconfig_observer_tests;
mod overload_monitor_tests;
mod party_objects_tests;
mod passkey_e2e_tests;
mod per_epoch_config_stress_tests;
mod protocol_version_tests;
mod randomness_tests;
mod readme;
mod reconfiguration_tests;
#[path = "rpc/main.rs"]
mod rpc;
mod shared_objects_tests;
mod shared_objects_version_tests;
mod simulator_tests;
mod soft_bundle_tests;
mod state_sync_resilience_tests;
mod traffic_control_tests;
mod transaction_api_tests;
mod transaction_deny_share_tests;
mod transaction_orchestrator_tests;
mod transaction_pool_tests;
mod transfer_to_object_tests;
mod zklogin_tests;
