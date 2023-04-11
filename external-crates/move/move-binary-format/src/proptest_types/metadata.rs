// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_core_types::metadata::Metadata;
use proptest::{
    arbitrary::any,
    collection::{btree_set, vec, SizeRange},
    strategy::Strategy,
};

// A metadata generation strategy.
#[derive(Clone, Debug)]
pub struct MetadataGen {
    blobs: Vec<Vec<u8>>,
}

impl MetadataGen {
    // Return a `Strategy` that builds Metadata vectors based on the given size.
    pub fn strategy(blob_size: impl Into<SizeRange>) -> impl Strategy<Value = Self> {
        btree_set(vec(any::<u8>(), 0..=20), blob_size).prop_map(|blobs| Self {
            blobs: blobs.into_iter().collect(),
        })
    }

    // Return the metadata
    pub fn metadata(self) -> Vec<Metadata> {
        let mut metadata = vec![];
        for blob in self.blobs {
            metadata.push(Metadata {
                key: blob.clone(),
                value: blob,
            })
        }
        metadata
    }
}
