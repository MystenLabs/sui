// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod node;

#[cfg(msim)]
#[path = "tests/simtests.rs"]
mod simtests;
