// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, RwLock};

use simulacrum::Simulacrum;

#[derive(Clone)]
pub struct SharedSimulacrum {
    inner: Arc<RwLock<Simulacrum>>,
}

impl SharedSimulacrum {
    pub fn inner(&self) -> std::sync::RwLockReadGuard<'_, Simulacrum> {
        self.inner.read().unwrap()
    }

    pub fn inner_mut(&self) -> std::sync::RwLockWriteGuard<'_, Simulacrum> {
        self.inner.write().unwrap()
    }
}

impl From<Simulacrum> for SharedSimulacrum {
    fn from(value: Simulacrum) -> Self {
        Self {
            inner: Arc::new(RwLock::new(value)),
        }
    }
}
