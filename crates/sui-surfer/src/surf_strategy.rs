// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use tokio::sync::watch;

use crate::surfer_state::{EntryFunction, SurferState};

#[async_trait]
pub trait SurfStrategy: Send + Sync + 'static {
    /// Given a state and a list of callable Move entry functions,
    /// explore them for a while, and eventually return. It's important that it
    /// eventually returns just so that the runtime can perform a few tasks
    /// such as checking whether to exit, or to sync some global state if need to.
    async fn surf_for_a_while(
        &mut self,
        state: &mut SurferState,
        entry_functions: Vec<EntryFunction>,
        exit: &watch::Receiver<()>,
    );
}
