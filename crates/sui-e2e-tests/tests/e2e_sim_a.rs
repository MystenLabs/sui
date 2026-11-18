// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The simulator tests build into a few binaries instead of one per file, because each test
// binary links the whole dependency graph. They are split so that they compile in parallel.
#![cfg(msim)]

#[path = "rpc/main.rs"]
mod rpc;
