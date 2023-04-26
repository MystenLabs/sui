// Copyright (c) Mysten Labs, Inc.
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
use std::future::Future;
use std::sync::Arc;

/// Wrap SuiNode to allow correct access to SuiNode in simulator tests.
pub struct SuiNodeHandle(Option<Arc<SuiNode>>);

impl SuiNodeHandle {
    pub fn new(node: Arc<SuiNode>) -> Self {
        Self(Some(node))
    }

    fn inner(&self) -> &Arc<SuiNode> {
        self.0.as_ref().unwrap()
    }

    pub fn with<T>(&self, cb: impl FnOnce(&SuiNode) -> T) -> T {
        let _guard = self.guard();
        cb(self.inner())
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
        cb(self.inner()).await
    }
}

#[cfg(msim)]
impl SuiNodeHandle {
    fn guard(&self) -> sui_simulator::runtime::NodeEnterGuard {
        self.inner().sim_node.enter_node()
    }

    pub async fn with_async<'a, F, R, T>(&'a self, cb: F) -> T
    where
        F: FnOnce(&'a SuiNode) -> R,
        R: Future<Output = T>,
    {
        let fut = cb(self.0.as_ref().unwrap());
        self.inner().sim_node.await_future_in_node(fut).await
    }
}

#[cfg(msim)]
impl Drop for SuiNodeHandle {
    fn drop(&mut self) {
        let node_id = self.inner().sim_node.id();
        // Shut down the sim node, but only if we were the last holder of a reference to the sui
        // node.
        let sui_node_arc = self.0.take().unwrap();
        let sui_node = Arc::downgrade(&sui_node_arc);
        drop(sui_node_arc);
        if sui_node.upgrade().is_none() {
            sui_simulator::runtime::Handle::try_current().map(|h| h.delete_node(node_id));
        }
    }
}

impl From<Arc<SuiNode>> for SuiNodeHandle {
    fn from(node: Arc<SuiNode>) -> Self {
        SuiNodeHandle::new(node)
    }
}
