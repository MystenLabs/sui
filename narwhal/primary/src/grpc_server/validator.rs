// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::time::Duration;

use crate::{block_waiter::GetBlockResponse, BlockCommand, BlockRemoverCommand};
use tokio::{
    sync::{mpsc::channel, mpsc::Sender, oneshot},
    time::timeout,
};
use tonic::{Request, Response, Status};
use types::{
    BatchMessageProto, BlockError, BlockRemoverErrorKind, CertificateDigest,
    CertificateDigestProto, CollectionRetrievalResult, Empty, GetCollectionsRequest,
    GetCollectionsResponse, RemoveCollectionsRequest, Validator,
};

#[derive(Debug)]
pub struct NarwhalValidator {
    tx_get_block_commands: Sender<BlockCommand>,
    tx_block_removal_commands: Sender<BlockRemoverCommand>,
    get_collections_timeout: Duration,
    remove_collections_timeout: Duration,
}

impl NarwhalValidator {
    pub fn new(
        tx_get_block_commands: Sender<BlockCommand>,
        tx_block_removal_commands: Sender<BlockRemoverCommand>,
        get_collections_timeout: Duration,
        remove_collections_timeout: Duration,
    ) -> Self {
        Self {
            tx_get_block_commands,
            tx_block_removal_commands,
            get_collections_timeout,
            remove_collections_timeout,
        }
    }
}

#[tonic::async_trait]
impl Validator for NarwhalValidator {
    async fn remove_collections(
        &self,
        request: Request<RemoveCollectionsRequest>,
    ) -> Result<Response<Empty>, Status> {
        let collection_ids = request.into_inner().collection_ids;
        let remove_collections_response = if !collection_ids.is_empty() {
            let (tx_remove_block, mut rx_remove_block) = channel(1);
            let ids = parse_certificate_digests(collection_ids)?;
            self.tx_block_removal_commands
                .send(BlockRemoverCommand::RemoveBlocks {
                    ids,
                    sender: tx_remove_block,
                })
                .await
                .map_err(|err| Status::internal(format!("Send Error: {err:?}")))?;
            match timeout(self.remove_collections_timeout, rx_remove_block.recv())
                .await
                .map_err(|_err| Status::internal("Timeout, no result has been received in time"))?
            {
                Some(result) => match result {
                    Ok(_) => Ok(Empty {}),
                    Err(remove_block_error)
                        if remove_block_error.error == BlockRemoverErrorKind::Timeout =>
                    {
                        Err(Status::internal(
                            "Timeout, no result has been received in time",
                        ))
                    }
                    Err(remove_block_error) => Err(Status::internal(format!(
                        "Removal Error: {:?}",
                        remove_block_error.error
                    ))),
                },
                None => Err(Status::internal(
                    "Removal channel closed, no result has been received.",
                )),
            }
        } else {
            Err(Status::invalid_argument(
                "Attemped to remove no collections!",
            ))
        };
        remove_collections_response.map(Response::new)
    }

    async fn get_collections(
        &self,
        request: Request<GetCollectionsRequest>,
    ) -> Result<Response<GetCollectionsResponse>, Status> {
        let collection_ids = request.into_inner().collection_ids;
        let get_collections_response = if !collection_ids.is_empty() {
            let (tx_get_blocks, rx_get_blocks) = oneshot::channel();
            let ids = parse_certificate_digests(collection_ids)?;
            self.tx_get_block_commands
                .send(BlockCommand::GetBlocks {
                    ids,
                    sender: tx_get_blocks,
                })
                .await
                .map_err(|err| Status::internal(format!("Send Error: {err:?}")))?;
            match timeout(self.get_collections_timeout, rx_get_blocks)
                .await
                .map_err(|_err| Status::internal("Timeout, no result has been received in time"))?
                .map_err(|_err| Status::internal("Fetch Error, no result has been received"))?
            {
                Ok(blocks_response) => {
                    let mut retrieval_results = vec![];
                    for block_result in blocks_response.blocks {
                        retrieval_results.extend(get_collection_retrieval_results(block_result));
                    }
                    Ok(GetCollectionsResponse {
                        result: retrieval_results,
                    })
                }
                Err(err) => Err(Status::internal(format!(
                    "Expected to receive a successful get blocks result, instead got error: {err:?}",
                ))),
            }
        } else {
            Err(Status::invalid_argument(
                "Attemped fetch of no collections!",
            ))
        };
        get_collections_response.map(Response::new)
    }
}

fn get_collection_retrieval_results(
    block_result: Result<GetBlockResponse, BlockError>,
) -> Vec<CollectionRetrievalResult> {
    match block_result {
        Ok(block_response) => {
            let mut collection_retrieval_results = vec![];
            for batch in block_response.batches {
                collection_retrieval_results.push(CollectionRetrievalResult {
                    retrieval_result: Some(types::RetrievalResult::Batch(BatchMessageProto::from(
                        batch,
                    ))),
                });
            }
            collection_retrieval_results
        }
        Err(block_error) => {
            vec![CollectionRetrievalResult {
                retrieval_result: Some(types::RetrievalResult::Error(block_error.into())),
            }]
        }
    }
}

fn parse_certificate_digests(
    collection_ids: Vec<CertificateDigestProto>,
) -> Result<Vec<CertificateDigest>, Status> {
    let mut ids = vec![];
    for collection_id in collection_ids {
        ids.push(
            collection_id.try_into().map_err(|err| {
                Status::invalid_argument(format!("Could not serialize: {:?}", err))
            })?,
        );
    }
    Ok(ids)
}
