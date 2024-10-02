// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::packages;
use crate::types::IndexedPackage;

use diesel::prelude::*;

#[derive(Queryable, Insertable, Selectable, Clone, Debug, Identifiable)]
#[diesel(table_name = packages, primary_key(package_id))]
pub struct StoredPackage {
    pub package_id: Vec<u8>,
    pub original_id: Vec<u8>,
    pub package_version: i64,
    pub move_package: Vec<u8>,
    pub checkpoint_sequence_number: i64,
}

impl From<IndexedPackage> for StoredPackage {
    fn from(p: IndexedPackage) -> Self {
        Self {
            package_id: p.package_id.to_vec(),
            original_id: p.move_package.original_package_id().to_vec(),
            package_version: p.move_package.version().value() as i64,
            move_package: bcs::to_bytes(&p.move_package).unwrap(),
            checkpoint_sequence_number: p.checkpoint_sequence_number as i64,
        }
    }
}
