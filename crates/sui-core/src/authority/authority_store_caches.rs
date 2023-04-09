// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use moka::future::{Cache, CacheBuilder};
use move_core_types::language_storage::StructTag;
use sui_json_rpc_types::Balance;
use sui_types::base_types::SuiAddress;

pub struct AuthorityStoreCaches {
    pub balance: Cache<SuiAddress, Arc<HashMap<StructTag, Balance>>>,
}

impl AuthorityStoreCaches {
    fn new() -> Self {
        AuthorityStoreCaches {
            balance: Cache::new(100_000),
        }
    }
    fn get_balance(&self, owner: SuiAddress, coin_type: StructTag) -> Balance {
        self.balance.get_with()
    }
}