// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ingestion::client::{FetchError, FetchResult, IngestionClientTrait};
use axum::body::Bytes;
use std::path::PathBuf;

// FIXME: To productionize this, we need to add garbage collection to remove old checkpoint files.

pub struct LocalIngestionClient {
    path: PathBuf,
}

impl LocalIngestionClient {
    pub fn new(path: PathBuf) -> Self {
        LocalIngestionClient { path }
    }
}

#[async_trait::async_trait]
impl IngestionClientTrait for LocalIngestionClient {
    async fn fetch(&self, checkpoint: u64) -> FetchResult {
        let path = self.path.join(format!("{}.chk", checkpoint));
        let bytes = tokio::fs::read(path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                FetchError::NotFound
            } else {
                FetchError::Transient {
                    reason: "io_error",
                    error: e.into(),
                }
            }
        })?;
        Ok(Bytes::from(bytes))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::ingestion::client::IngestionClient;
    use crate::ingestion::test_utils::test_checkpoint_data;
    use crate::metrics::tests::test_metrics;
    use sui_storage::blob::{Blob, BlobEncoding};
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn local_test_fetch() {
        let tempdir = tempfile::tempdir().unwrap().into_path();
        let path = tempdir.join("1.chk");
        let test_checkpoint = test_checkpoint_data(1);
        tokio::fs::write(&path, &test_checkpoint).await.unwrap();

        let local_client = IngestionClient::new_local(tempdir, test_metrics());
        let checkpoint = local_client
            .fetch(1, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            Blob::encode(&*checkpoint, BlobEncoding::Bcs)
                .unwrap()
                .to_bytes(),
            test_checkpoint
        );
    }
}
