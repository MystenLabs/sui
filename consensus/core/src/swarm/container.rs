// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use super::node::NodeConfig;

pub(crate) struct AuthorityNodeContainer {
}

impl AuthorityNodeContainer {
    /// Spawn a new Node.
    pub async fn spawn(
        config: NodeConfig
    ) -> Self {
        info!(index =% config.authority_index, "starting in-memory node non-sim");
        Self {
            
        }
    }

    pub fn is_alive(&self) -> bool {
        true
    }
}