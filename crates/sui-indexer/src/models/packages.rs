// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::packages;
use crate::types::IndexedPackage;

use diesel::prelude::*;

#[derive(Queryable, Insertable, Selectable, Clone, Debug, Identifiable)]
#[diesel(table_name = packages, primary_key(package_id))]
pub struct StoredPackage {
    pub package_id: Vec<u8>,
    pub move_package: Vec<u8>,
}

impl From<IndexedPackage> for StoredPackage {
    fn from(p: IndexedPackage) -> Self {
        Self {
            package_id: p.package_id.to_vec(),
            move_package: bcs::to_bytes(&p.move_package).unwrap(),
        }
    }
}
