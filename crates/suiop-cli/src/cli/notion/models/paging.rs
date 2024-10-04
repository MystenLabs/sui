// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Clone)]
#[serde(transparent)]
pub struct PagingCursor(String);

#[derive(Serialize, Debug, Eq, PartialEq, Default, Clone)]
pub struct Paging {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_cursor: Option<PagingCursor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_size: Option<u8>,
}

pub trait Pageable {
    fn start_from(self, starting_point: Option<PagingCursor>) -> Self;
}
