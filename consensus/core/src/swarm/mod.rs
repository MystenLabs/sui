// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod node;

#[cfg(msim)]
#[path = "./container-sim.rs"]
mod container;

#[cfg(not(msim))]
#[path = "./container.rs"]
mod container;


