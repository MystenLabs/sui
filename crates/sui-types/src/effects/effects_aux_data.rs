// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::ObjectRef;
use crate::crypto::default_hash;
use crate::digests::{EffectsAuxDataDigest, TransactionDigest};
use anyhow::anyhow;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Hash)]
pub struct EffectsAuxData<T> {
    tx_digest: TransactionDigest,
    version: u64,
    inner: T,
}

/// Separating storage type and in-memory type allows for customized serde.
pub type EffectsAuxDataInStorage = EffectsAuxData<Vec<u8>>;
pub type EffectsAuxDataInMemory = EffectsAuxData<Vec<EffectsAuxDataEntry>>;

#[derive(Serialize, Deserialize)]
pub enum EffectsAuxDataEntry {
    // Could either be children of read-only shared objects that are loaded at runtime,
    // or objects that are successfully received at runtime but didn't materialize in the end (due to
    // execution failure).
    ReadOnlyRuntimeObjects(Vec<ObjectRef>),
}

impl EffectsAuxDataInStorage {
    pub fn deserialize(self) -> anyhow::Result<EffectsAuxDataInMemory> {
        let inner = match self.version {
            1 => Self::deserialize_v1(&self.inner),
            _ => Err(anyhow!(
                "Unsupported EffectsAuxData version: {}",
                self.version
            )),
        }?;
        Ok(EffectsAuxDataInMemory {
            tx_digest: self.tx_digest,
            version: self.version,
            inner,
        })
    }

    pub fn digest(&self) -> EffectsAuxDataDigest {
        EffectsAuxDataDigest::new(default_hash(self))
    }

    fn deserialize_v1(inner: &[u8]) -> anyhow::Result<Vec<EffectsAuxDataEntry>> {
        bincode::deserialize(inner).map_err(|e| e.into())
    }
}

impl EffectsAuxDataInMemory {
    pub fn new(tx_digest: TransactionDigest, inner: Vec<EffectsAuxDataEntry>) -> Self {
        Self {
            tx_digest,
            version: 1,
            inner,
        }
    }

    pub fn serialize(&self) -> anyhow::Result<EffectsAuxDataInStorage> {
        let inner = match self.version {
            1 => Self::serialize_v1(&self.inner),
            _ => Err(anyhow!(
                "Unsupported EffectsAuxData version: {}",
                self.version
            )),
        }?;
        Ok(EffectsAuxDataInStorage {
            tx_digest: self.tx_digest,
            version: self.version,
            inner,
        })
    }

    fn serialize_v1(inner: &[EffectsAuxDataEntry]) -> anyhow::Result<Vec<u8>> {
        bincode::serialize(inner).map_err(|e| e.into())
    }
}
