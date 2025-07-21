// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use std::sync::Arc;

use crate::config::RpcConfig;
use crate::schema::Schema;
use crate::store::Store;

/// State exposed to RPC service implementations.
#[derive(Clone)]
pub(crate) struct State {
    /// Access to the database.
    pub store: Store<Schema>,

    /// Configuration
    pub config: Arc<RpcConfig>,
}
