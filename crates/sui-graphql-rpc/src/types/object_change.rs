// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::object::Object;
use async_graphql::SimpleObject;

#[derive(PartialEq, Eq, Clone, SimpleObject)]
#[allow(dead_code)]
pub(crate) struct ObjectChange {
    pub input_state: Option<Object>,
    pub output_state: Option<Object>,
    pub id_created: Option<bool>,
    pub id_deleted: Option<bool>,
}
