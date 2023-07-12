// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::packages;

use diesel::prelude::*;

use crate::models::objects::NamedBcsBytes;
use sui_json_rpc_types::SuiRawMovePackage;
use sui_types::base_types::SuiAddress;

#[derive(Queryable, Insertable, Clone, Debug, Identifiable)]
#[diesel(table_name = packages, primary_key(package_id, version))]
pub struct Package {
    pub package_id: String,
    pub version: i64,
    pub author: String,
    pub data: Vec<NamedBcsBytes>,
}

impl Package {
    pub fn try_from(sender: SuiAddress, package: &SuiRawMovePackage) -> Result<Self, IndexerError> {
        Ok(Self {
            package_id: package.id.to_string(),
            version: package.version.value() as i64,
            author: sender.to_string(),
            data: package
                .module_map
                .clone()
                .into_iter()
                .map(|(k, v)| NamedBcsBytes(k, v))
                .collect(),
        })
    }
}
