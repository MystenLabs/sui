// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The simulator tests build into a few binaries instead of one per file, because each test
// binary links the whole dependency graph. They are split so that they compile in parallel.
#![cfg(msim)]

mod address_balance_tests;
mod admission_queue_tests;
mod authenticated_events_client_test;
mod checkpoint_rate_tests;
mod db_shell_tests;
mod dynamic_committee_tests;
mod gasless_tests;
mod get_object_inclusion_proof_tests;
mod multisig_tests;
mod object_deletion_tests;
mod onsite_reconfig_observer_tests;
mod overload_monitor_tests;
mod passkey_e2e_tests;
mod per_epoch_config_stress_tests;
mod randomness_tests;
mod reconfiguration_tests;
mod shared_objects_tests;
mod simulator_tests;
mod soft_bundle_tests;
mod state_sync_resilience_tests;
mod transaction_api_tests;
mod transaction_deny_share_tests;
mod transaction_pool_tests;
mod transfer_to_object_tests;
mod zklogin_tests;
