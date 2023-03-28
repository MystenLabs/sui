// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::{sync::Arc, time::Duration};

use crate::{
    block_synchronizer::handler::Handler, block_waiter::GetBlockResponse, BlockRemover, BlockWaiter,
};
use consensus::dag::Dag;
use tokio::time::timeout;
use tonic::{Request, Response, Status};
use types::{
    BatchAPI, BlockError, CertificateDigest, CertificateDigestProto, Collection,
    CollectionRetrievalResult, Empty, GetCollectionsRequest, GetCollectionsResponse,
    ReadCausalRequest, ReadCausalResponse, RemoveCollectionsRequest, TransactionProto, Validator,
};

pub struct NarwhalValidator<SynchronizerHandler: Handler + Send + Sync + 'static> {
    block_waiter: BlockWaiter<SynchronizerHandler>,
    block_remover: BlockRemover,
    get_collections_timeout: Duration,
    remove_collections_timeout: Duration,
    block_synchronizer_handler: Arc<SynchronizerHandler>,
    dag: Option<Arc<Dag>>,
}

impl<SynchronizerHandler: Handler + Send + Sync + 'static> NarwhalValidator<SynchronizerHandler> {
    pub fn new(
        block_waiter: BlockWaiter<SynchronizerHandler>,
        block_remover: BlockRemover,
        get_collections_timeout: Duration,
        remove_collections_timeout: Duration,
        block_synchronizer_handler: Arc<SynchronizerHandler>,
        dag: Option<Arc<Dag>>,
    ) -> Self {
        Self {
            block_waiter,
            block_remover,
            get_collections_timeout,
            remove_collections_timeout,
            block_synchronizer_handler,
            dag,
        }
    }
}

#[tonic::async_trait]
impl<SynchronizerHandler: Handler + Send + Sync + 'static> Validator
    for NarwhalValidator<SynchronizerHandler>
{
    async fn read_causal(
        &self,
        request: Request<ReadCausalRequest>,
    ) -> Result<Response<ReadCausalResponse>, Status> {
        let collection_id = request
            .into_inner()
            .collection_id
            .ok_or_else(|| Status::invalid_argument("No collection id has been provided"))?;
        let ids = parse_certificate_digests(vec![collection_id])?;

        let block_header_results = self
            .block_synchronizer_handler
            .get_and_synchronize_block_headers(ids.clone())
            .await;

        for result in block_header_results {
            if let Err(err) = result {
                return Err(Status::internal(format!(
                    "Error when trying to synchronize block headers: {:?}",
                    err
                )));
            }
        }

        if let Some(dag) = &self.dag {
            let result = match dag.read_causal(ids[0]).await {
                Ok(digests) => Ok(ReadCausalResponse {
                    collection_ids: digests.into_iter().map(Into::into).collect(),
                }),
                Err(err) => Err(Status::internal(format!("Couldn't read causal: {err}"))),
            };
            return result.map(Response::new);
        }
        Err(Status::internal("Dag does not exist"))
    }

    async fn remove_collections(
        &self,
        request: Request<RemoveCollectionsRequest>,
    ) -> Result<Response<Empty>, Status> {
        let collection_ids = request.into_inner().collection_ids;
        let remove_collections_response = if !collection_ids.is_empty() {
            let ids = parse_certificate_digests(collection_ids)?;
            match timeout(
                self.remove_collections_timeout,
                self.block_remover.remove_blocks(ids),
            )
            .await
            .map_err(|_err| Status::internal("Timeout, no result has been received in time"))?
            {
                Ok(_) => Ok(Empty {}),
                Err(e) => Err(Status::internal(format!("Removal Error: {e:?}"))),
            }
        } else {
            Err(Status::invalid_argument(
                "Attempted to remove no collections!",
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
            let ids = parse_certificate_digests(collection_ids)?;
            let blocks_response = timeout(self.get_collections_timeout, self.block_waiter.get_blocks(ids))
                .await
                .map_err(|_err| Status::internal("Timeout, no result has been received in time"))?
                .map_err(|err| Status::internal(format!(
                    "Expected to receive a successful get blocks result, instead got error: {err:?}",
                )))?;
            let result: Vec<_> = blocks_response
                .blocks
                .into_iter()
                .map(get_collection_retrieval_results)
                .collect();
            Ok(GetCollectionsResponse { result })
        } else {
            Err(Status::invalid_argument(
                "Attempted fetch of no collections!",
            ))
        };
        get_collections_response.map(Response::new)
    }
}

fn get_collection_retrieval_results(
    block_result: Result<GetBlockResponse, BlockError>,
) -> CollectionRetrievalResult {
    match block_result {
        Ok(block_response) => {
            let mut transactions = vec![];
            for batch in block_response.batches {
                transactions.extend(
                    batch
                        .batch
                        .transactions()
                        .clone()
                        .into_iter()
                        .map(Into::into)
                        .collect::<Vec<TransactionProto>>(),
                )
            }
            CollectionRetrievalResult {
                retrieval_result: Some(types::RetrievalResult::Collection(Collection {
                    id: Some(CertificateDigestProto::from(block_response.digest)),
                    transactions,
                })),
            }
        }
        Err(block_error) => CollectionRetrievalResult {
            retrieval_result: Some(types::RetrievalResult::Error(block_error.into())),
        },
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
