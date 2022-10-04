// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! SuiNodeHandle wraps SuiNode in a way suitable for access by test code.
//!
//! When starting a SuiNode directly, in a test (as opposed to using Swarm), the node may be
//! running inside of a simulator node. It is therefore a mistake to do something like:
//!
//! ```ignore
//!     use test_utils::authority::{start_node, spawn_checkpoint_processes};
//!
//!     let node = start_node(config, registry).await;
//!     spawn_checkpoint_processes(config, &[node]).await;
//! ```
//!
//! Because this would cause the checkpointing processes to be running inside the current
//! simulator node rather than the node in which the SuiNode is running.
//!
//! SuiNodeHandle provides an easy way to do the right thing here:
//!
//! ```ignore
//!     let node_handle = start_node(config, registry).await;
//!     node_handle.with_async(|sui_node| async move {
//!         spawn_checkpoint_processes(config, &[sui_node]).await;
//!     });
//! ```
//!
//! Code executed inside of with or with_async will run in the context of the simulator node.
//! This allows tests to break the simulator abstraction and magically mutate or inspect state that
//! is conceptually running on a different "machine", but without producing extremely confusing
//! behavior that might result otherwise. (For instance, any network connection that is initiated
//! from a task spawned from within a with or with_async will appear to originate from the correct
//! simulator node.
//!
//! It is possible to exfiltrate state:
//!
//! ```ignore
//!    let state = node_handle.with(|sui_node| sui_node.state);
//!    // DO NOT DO THIS!
//!    do_stuff_with_state(state)
//! ```
//!
//! We can't prevent this completely, but we can at least make the right way the easy way.

use super::SuiNode;
use anyhow::Result;
use std::future::Future;

/// Wrap SuiNode to allow correct access to SuiNode in simulator tests.
pub struct SuiNodeHandle(SuiNode);

impl SuiNodeHandle {
    pub fn new(node: SuiNode) -> Self {
        Self(node)
    }

    pub fn with<T>(&self, cb: impl FnOnce(&SuiNode) -> T) -> T {
        let _guard = self.guard();
        cb(&self.0)
    }

    pub fn with_mut(&mut self, cb: impl FnOnce(&mut SuiNode)) {
        let _guard = self.guard();
        cb(&mut self.0);
    }

    pub async fn wait(self) -> Result<()> {
        self.0.wait().await
    }
}

#[cfg(not(msim))]
impl SuiNodeHandle {
    // Must return something to silence lints above at `let _guard = ...`
    fn guard(&self) -> u32 {
        0
    }

    pub async fn with_async<'a, F, R, T>(&'a self, cb: F) -> T
    where
        F: FnOnce(&'a SuiNode) -> R,
        R: Future<Output = T>,
    {
        cb(&self.0).await
    }

    pub async fn with_mut_async<'a, F, R, T>(&'a mut self, cb: F) -> T
    where
        F: FnOnce(&'a mut SuiNode) -> R,
        R: Future<Output = T>,
    {
        cb(&mut self.0).await
    }
}

#[cfg(msim)]
impl SuiNodeHandle {
    fn guard(&self) -> sui_simulator::runtime::NodeEnterGuard {
        self.0.sim_node.enter_node()
    }

    pub async fn with_async<'a, F, R, T>(&'a self, cb: F) -> T
    where
        F: FnOnce(&'a SuiNode) -> R,
        R: Future<Output = T>,
    {
        let fut = cb(&self.0);
        self.0.sim_node.await_future_in_node(fut).await
    }

    pub async fn with_mut_async<'a, F, R, T>(&'a mut self, cb: F) -> T
    where
        F: FnOnce(&'a mut SuiNode) -> R,
        R: Future<Output = T>,
    {
        let node_clone = self.0.sim_node.clone();
        let fut = cb(&mut self.0);
        node_clone.await_future_in_node(fut).await
    }
}

impl From<SuiNode> for SuiNodeHandle {
    fn from(node: SuiNode) -> Self {
        SuiNodeHandle::new(node)
    }
}
