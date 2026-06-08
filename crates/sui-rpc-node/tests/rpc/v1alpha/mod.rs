// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests against the `sui.rpc.consistent.v1alpha` surface.
//! Mirrors the alt-consistent-store's e2e coverage but plays
//! against a `LocalCluster`-managed `sui-rpc-node` instead of
//! the older `start_consistent_store` binary.

mod consistent_service;
