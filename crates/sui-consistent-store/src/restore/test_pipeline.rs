// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Shared test fixtures for the `restore` module: a one-CF
//! `(ObjectID -> version)` schema plus a [`Restore`] pipeline that
//! populates it. Used by both the trait-level tests in
//! [`super::tests`] and the driver tests in
//! [`super::driver::tests`].

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Buf;
use bytes::BufMut;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::types::full_checkpoint_content::Checkpoint;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;
use tempfile::TempDir;

use crate::Batch;
use crate::CfDescriptor;
use crate::Db;
use crate::DbMap;
use crate::DbOptions;
use crate::Decode;
use crate::Encode;
use crate::Schema;
use crate::error::DecodeError;
use crate::error::EncodeError;
use crate::error::OpenError;
use crate::restore::Restore;

/// Big-endian `ObjectID` newtype, suitable as a typed key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ObjectIdKey([u8; ObjectID::LENGTH]);

impl ObjectIdKey {
    pub(crate) fn new(id: ObjectID) -> Self {
        Self(id.into_bytes())
    }
}

impl Encode for ObjectIdKey {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(&self.0);
        Ok(())
    }
}

impl Decode for ObjectIdKey {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != ObjectID::LENGTH {
            return Err(DecodeError::msg("unexpected ObjectIdKey length"));
        }
        let mut id = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id);
        Ok(Self(id))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct U64Be(pub u64);

impl Encode for U64Be {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(&self.0.to_be_bytes());
        Ok(())
    }
}

impl Decode for U64Be {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != 8 {
            return Err(DecodeError::msg("expected 8 bytes"));
        }
        Ok(Self(buf.get_u64()))
    }
}

#[derive(Debug)]
pub(crate) struct ObjectVersionSchema {
    pub(crate) versions: DbMap<ObjectIdKey, U64Be>,
}

impl Schema for ObjectVersionSchema {
    fn cfs(opts: &crate::options::CfOptionsResolver) -> Vec<CfDescriptor> {
        vec![CfDescriptor::new("versions", opts.options("versions"))]
    }

    fn open(db: &Db) -> Result<Self, OpenError> {
        Ok(Self {
            versions: DbMap::new(db.clone(), "versions")?,
        })
    }
}

/// Test pipeline: writes `(object_id -> version)` per object.
pub(crate) struct ObjectVersionPipeline;

#[async_trait]
impl Processor for ObjectVersionPipeline {
    const NAME: &'static str = "object_version";
    type Value = ();

    async fn process(&self, _: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        // Restore-only test pipeline; tip path not exercised.
        Ok(vec![])
    }
}

impl Restore for ObjectVersionPipeline {
    type Schema = ObjectVersionSchema;

    fn restore(
        &self,
        schema: &Self::Schema,
        object: &Object,
        batch: &mut Batch,
    ) -> anyhow::Result<()> {
        batch.put(
            &schema.versions,
            &ObjectIdKey::new(object.id()),
            &U64Be(object.version().value()),
        )?;
        Ok(())
    }
}

pub(crate) fn open() -> (TempDir, Db, ObjectVersionSchema) {
    let dir = TempDir::new().unwrap();
    let (db, schema) = Db::open::<ObjectVersionSchema>(dir.path(), DbOptions::default()).unwrap();
    (dir, db, schema)
}
