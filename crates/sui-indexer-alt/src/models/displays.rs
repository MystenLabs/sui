// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::prelude::*;

use crate::schema::sum_displays;

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = sum_displays, primary_key(object_type))]
pub struct StoredDisplay {
    pub object_type: Vec<u8>,
    pub display_id: Vec<u8>,
    pub display_version: i16,
    pub display: Vec<u8>,
}
