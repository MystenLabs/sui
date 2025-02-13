// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data::read_error::ReadError;
use sui_kvstore::BigTableClient;

pub struct BigtableReader(pub(crate) BigTableClient);

impl BigtableReader {
    pub(crate) async fn new(instance_id: String, credentials: String) -> Result<Self, ReadError> {
        std::env::set_var("GOOGLE_APPLICATION_CREDENTIALS", credentials);

        let client = BigTableClient::new_remote(instance_id, true, None)
            .await
            .map_err(|e| ReadError::BigtableCreate(e.into()))?;

        Ok(Self(client))
    }
}
