// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;
use sui_field_count::FieldCount;

use crate::schema::{kv_feature_flags, kv_protocol_configs};

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = kv_feature_flags)]
pub struct StoredFeatureFlag {
    pub protocol_version: i64,
    pub flag_name: String,
    pub flag_value: bool,
}

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = kv_protocol_configs)]
pub struct StoredProtocolConfig {
    pub protocol_version: i64,
    pub config_name: String,
    pub config_value: Option<String>,
}
