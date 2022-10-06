// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! An `in-memory`, or rather `in-process`, backend for building and managing Sui Networks that all
//! run inside the same process. Nodes are isolated from one another by each being run on their own
//! separate thread within their own `tokio` runtime. This enables the ability to properly shut
//! down a single node and ensure that all of its running tasks are also shut down, something that
//! is extremely difficult or down right impossible to do if all the nodes are running on the same
//! runtime.

mod node;
pub use node::{Node, RuntimeType};

mod swarm;
pub use swarm::{Swarm, SwarmBuilder};

#[cfg(msim)]
#[path = "./container-sim.rs"]
mod container;

#[cfg(not(msim))]
#[path = "./container.rs"]
mod container;
