// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::time::Duration;

use crate::block_waiter::GetBlockResponse;
use crate::BlockCommand;
use tokio::sync::oneshot;
use tokio::{sync::mpsc::Sender, time::timeout};
use tonic::{Request, Response, Status};
use types::{
    BatchMessageProto, BlockError, CollectionRetrievalResult, GetCollectionsRequest,
    GetCollectionsResponse, Validator,
};

#[derive(Debug)]
pub struct NarwhalValidator {
    tx_get_block_commands: Sender<BlockCommand>,
    get_collections_timeout: Duration,
}

impl NarwhalValidator {
    pub fn new(
        tx_get_block_commands: Sender<BlockCommand>,
        get_collections_timeout: Duration,
    ) -> Self {
        Self {
            tx_get_block_commands,
            get_collections_timeout,
        }
    }
}

#[tonic::async_trait]
impl Validator for NarwhalValidator {
    async fn get_collections(
        &self,
        request: Request<GetCollectionsRequest>,
    ) -> Result<Response<GetCollectionsResponse>, Status> {
        let collection_ids = request.into_inner().collection_ids;
        let get_collections_response = if !collection_ids.is_empty() {
            let (tx_get_blocks, rx_get_blocks) = oneshot::channel();
            let mut ids = vec![];
            for collection_id in collection_ids {
                ids.push(collection_id.try_into().map_err(|err| {
                    Status::invalid_argument(format!("Could not serialize: {:?}", err))
                })?);
            }
            self.tx_get_block_commands
                .send(BlockCommand::GetBlocks {
                    ids,
                    sender: tx_get_blocks,
                })
                .await
                .unwrap();
            match timeout(self.get_collections_timeout, rx_get_blocks).await {
                Ok(Ok(result)) => {
                    match result {
                        Ok(blocks_response) => {
                            let mut retrieval_results = vec![];
                            for block_result in blocks_response.blocks {
                                retrieval_results.extend(get_collection_retrieval_results(block_result));
                            }
                            Ok(GetCollectionsResponse {
                                result: retrieval_results,
                            })
                        },
                        Err(err) => {
                            Err(Status::internal(format!("Expected to receive a successful get blocks result, instead got error: {:?}", err)))
                        }
                    }
                }
                Ok(Err(_)) => Err(Status::internal(
                    "Fetch Error, no result has been received.",
                )),
                Err(_) => Err(Status::internal(
                    "Timeout, no result has been received in time",
                )),
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
