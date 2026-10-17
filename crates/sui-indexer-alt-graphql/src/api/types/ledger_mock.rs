// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! An in-process ledger service for unit tests of the List query paths. It records the
//! `start_checkpoint` of every `ListTransactions`/`ListEvents` request and fails each one with a
//! fixed status, delivered on the stream as the fullnode does.

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
    status: Status,
    start_checkpoints: Arc<Mutex<Vec<Option<u64>>>>,
}

impl FailingLedger {
    /// Serve a ledger that fails every List request with `status`, and a reader pointed at it.
    pub(crate) async fn start(status: Status) -> (Self, AlphaLedgerGrpcReader) {
        let ledger = Self {
            status,
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
        stream::iter([Err(self.status.clone())]).boxed()
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
