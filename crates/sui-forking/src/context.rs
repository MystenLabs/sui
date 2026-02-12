// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use rand::rngs::OsRng;
use tokio::sync::RwLock;

use crate::store::ForkingStore;
use simulacrum::Simulacrum;

#[derive(Clone)]
pub(crate) struct Context {
    pub simulacrum: Arc<RwLock<Simulacrum<OsRng, ForkingStore>>>,
    pub at_checkpoint: u64,
}
