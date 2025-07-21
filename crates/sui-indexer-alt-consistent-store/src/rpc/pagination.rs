// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_default_config::DefaultConfig;

#[DefaultConfig]
pub struct PaginationConfig {
    pub default_page_size: u32,
    pub max_page_size: u32,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            default_page_size: 50,
            max_page_size: 200,
        }
    }
}
