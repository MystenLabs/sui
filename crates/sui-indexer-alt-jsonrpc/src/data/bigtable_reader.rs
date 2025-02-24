// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data::error::Error;
use async_graphql::dataloader::DataLoader;
use sui_kvstore::BigTableClient;

/// A reader backed by Bigtable kv store.
/// In order to use this reader, the environment variable `GOOGLE_APPLICATION_CREDENTIALS`
/// must be set to the path of the credentials file.
#[derive(Clone)]
pub struct BigtableReader(pub(crate) BigTableClient);

impl BigtableReader {
    pub(crate) async fn new(instance_id: String) -> Result<Self, Error> {
        if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_err() {
            return Err(Error::BigtableCreate(anyhow::anyhow!(
                "Environment variable GOOGLE_APPLICATION_CREDENTIALS is not set"
            )));
        }
        let client = BigTableClient::new_remote(instance_id, true, None)
            .await
            .map_err(Error::BigtableCreate)?;
        Ok(Self(client))
    }

    /// Create a data loader backed by this reader.
    pub(crate) fn as_data_loader(&self) -> DataLoader<Self> {
        DataLoader::new(self.clone(), tokio::spawn)
    }
}
