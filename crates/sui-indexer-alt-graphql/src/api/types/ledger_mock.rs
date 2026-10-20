// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! An in-process ledger service for unit tests of the List query paths. It records the
//! `start_checkpoint` of every `ListTransactions`/`ListEvents` request and fails each one with
//! `OUT_OF_RANGE`, delivered on the stream as the fullnode does: with the fullnode's below-floor
//! message for a start below its floor, and its ahead-of-tip message otherwise.

use std::sync::Arc;
use std::sync::Mutex;

use futures::StreamExt;
use futures::stream;
use prometheus::Registry;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::ledger_grpc_reader::LedgerGrpcArgs;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerService;
use sui_rpc::proto::sui::rpc::v2::ledger_service_server::LedgerServiceServer;
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::Request;
use tonic::Response;
use tonic::Status;

type BoxStream<T> = std::pin::Pin<Box<dyn futures::Stream<Item = Result<T, Status>> + Send>>;

#[derive(Clone)]
pub(crate) struct FailingLedger {
    floor: u64,
    start_checkpoints: Arc<Mutex<Vec<Option<u64>>>>,
}

impl FailingLedger {
    /// Serve a ledger with retention floor `floor` that fails every List request, and a reader
    /// pointed at it.
    pub(crate) async fn start(floor: u64) -> (Self, AlphaLedgerGrpcReader) {
        let ledger = Self {
            floor,
            start_checkpoints: Arc::default(),
        };

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let service = LedgerServiceServer::new(ledger.clone());
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(service)
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .ok();
        });

        let args = LedgerGrpcArgs {
            ledger_grpc_statement_timeout_ms: None,
            ledger_grpc_max_decoding_message_size: 1024 * 1024,
            ledger_grpc_num_connections: 1,
        };
        let reader = AlphaLedgerGrpcReader::new(
            format!("http://{addr}").parse().unwrap(),
            args,
            None,
            &Registry::new(),
            None,
        )
        .await
        .unwrap();

        (ledger, reader)
    }

    /// The `start_checkpoint` of each List request received, in order.
    pub(crate) fn start_checkpoints(&self) -> Vec<Option<u64>> {
        self.start_checkpoints.lock().unwrap().clone()
    }

    fn fail<T: Send + 'static>(&self, start_checkpoint: Option<u64>) -> BoxStream<T> {
        self.start_checkpoints
            .lock()
            .unwrap()
            .push(start_checkpoint);
        let status = match start_checkpoint {
            Some(start) if start < self.floor => Status::out_of_range(format!(
                "requested data below earliest available; lowest available checkpoint is {}",
                self.floor
            )),
            _ => Status::out_of_range(
                "requested data above latest available; cursor is at checkpoint 0, and only \
                 checkpoints below 0 are available",
            ),
        };
        stream::iter([Err(status)]).boxed()
    }
}

#[tonic::async_trait]
impl LedgerService for FailingLedger {
    async fn list_transactions(
        &self,
        request: Request<proto::ListTransactionsRequest>,
    ) -> Result<Response<BoxStream<proto::ListTransactionsResponse>>, Status> {
        Ok(Response::new(
            self.fail(request.into_inner().start_checkpoint),
        ))
    }

    async fn list_events(
        &self,
        request: Request<proto::ListEventsRequest>,
    ) -> Result<Response<BoxStream<proto::ListEventsResponse>>, Status> {
        Ok(Response::new(
            self.fail(request.into_inner().start_checkpoint),
        ))
    }
}
