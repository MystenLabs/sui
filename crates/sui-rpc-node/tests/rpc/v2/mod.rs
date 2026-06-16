// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests against the `sui_rpc::proto::sui::rpc::v2` surface.
//! Layout mirrors the equivalent module under
//! `sui-e2e-tests/tests/rpc/v2/`; submodules under each service
//! hold one test file per gRPC method.

mod ledger_service;
mod move_package_service;
mod state_service;
mod subscription_service;
mod unchanged_loaded_runtime_objects;
