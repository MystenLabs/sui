// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use anyhow::bail;
use tracing::info;

use sui_graphql::CheckpointResponse;
use sui_graphql::Client;
use sui_types::crypto::AggregateAuthoritySignature;
use sui_types::crypto::AuthorityQuorumSignInfo;
use sui_types::message_envelope::Envelope;
use sui_types::messages_checkpoint::VerifiedCheckpoint;

/// Trait abstracting network data fetching, allowing tests to provide mock implementations.
#[async_trait::async_trait]
pub trait ForkDataProvider: Send + Sync {
    /// Fetch a checkpoint. If `sequence_number` is `None`, fetch the latest checkpoint.
    async fn fetch_checkpoint(&self, sequence_number: Option<u64>) -> Result<VerifiedCheckpoint>;

    async fn fetch_protocol_version(&self) -> Result<u64>;
}

pub struct GraphQLQueryClient {
    client: Client,
}

impl GraphQLQueryClient {
    pub fn new(endpoint: &str) -> Result<Self> {
        let client = Client::new(endpoint)?;
        Ok(Self { client })
    }

    fn convert_checkpoint(response: CheckpointResponse) -> Result<VerifiedCheckpoint> {
        let summary = response.summary;
        let sequence_number = summary.sequence_number;
        let dummy_sig = AuthorityQuorumSignInfo {
            epoch: summary.epoch,
            signature: AggregateAuthoritySignature::default(),
            signers_map: roaring::RoaringBitmap::new(),
        };
        let certified = Envelope::new_from_data_and_sig(summary.try_into()?, dummy_sig);
        info!("Fetched checkpoint: {}", sequence_number);
        Ok(VerifiedCheckpoint::new_unchecked(certified))
    }
}

#[async_trait::async_trait]
impl ForkDataProvider for GraphQLQueryClient {
    async fn fetch_checkpoint(&self, sequence_number: Option<u64>) -> Result<VerifiedCheckpoint> {
        let response = self
            .client
            .get_checkpoint(sequence_number)
            .await
            .map_err(anyhow::Error::from)?;

        match response {
            Some(checkpoint) => Self::convert_checkpoint(checkpoint),
            None => bail!("Failed to fetch checkpoint {sequence_number:?}"),
        }
    }

    async fn fetch_protocol_version(&self) -> Result<u64> {
        Ok(self.client.protocol_version().await?)
    }
}

#[cfg(test)]
pub(crate) struct MockNetworkDataClient {
    pub checkpoint: VerifiedCheckpoint,
    pub protocol_version: u64,
}

#[cfg(test)]
#[async_trait::async_trait]
impl ForkDataProvider for MockNetworkDataClient {
    async fn fetch_checkpoint(&self, _sequence_number: Option<u64>) -> Result<VerifiedCheckpoint> {
        Ok(self.checkpoint.clone())
    }

    async fn fetch_protocol_version(&self) -> Result<u64> {
        Ok(self.protocol_version)
    }
}
