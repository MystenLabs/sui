use anemo::rpc::Status;
use arc_swap::ArcSwapOption;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::instrument;
use types::{
    FetchCertificatesRequest, FetchCertificatesResponse, GetCertificatesRequest,
    GetCertificatesResponse, PayloadAvailabilityRequest, PayloadAvailabilityResponse,
    PrimaryMessage, PrimaryToPrimary, RequestVoteRequest, RequestVoteResponse, WorkerInfoResponse,
    WorkerOthersBatchMessage, WorkerOurBatchMessage, WorkerToPrimary,
};

pub use crate::handlers::primary::PrimaryReceiverController;
pub use crate::handlers::worker::WorkerReceiverController;

mod primary;
mod worker;

/// Defines how the network receiver handles incoming primary messages.
#[derive(Clone)]
pub struct PrimaryReceiverHandler {
    pub controller: Arc<ArcSwapOption<PrimaryReceiverController>>,
}

#[allow(clippy::result_large_err)]
impl PrimaryReceiverHandler {
    pub fn new(controller: Arc<ArcSwapOption<PrimaryReceiverController>>) -> Self {
        PrimaryReceiverHandler { controller }
    }
}

#[async_trait]
impl PrimaryToPrimary for PrimaryReceiverHandler {
    async fn send_message(
        &self,
        request: anemo::Request<PrimaryMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        self.controller
            .load_full()
            .ok_or_else(|| Status::internal("Service not ready"))?
            .send_message(request)
            .await
    }

    async fn request_vote(
        &self,
        request: anemo::Request<RequestVoteRequest>,
    ) -> Result<anemo::Response<RequestVoteResponse>, anemo::rpc::Status> {
        self.controller
            .load_full()
            .ok_or_else(|| Status::internal("Service not ready"))?
            .request_vote(request)
            .await
    }

    async fn get_certificates(
        &self,
        request: anemo::Request<GetCertificatesRequest>,
    ) -> Result<anemo::Response<GetCertificatesResponse>, anemo::rpc::Status> {
        self.controller
            .load_full()
            .ok_or_else(|| Status::internal("Service not ready"))?
            .get_certificates(request)
            .await
    }

    #[instrument(level = "debug", skip_all, peer = ?request.peer_id())]
    async fn fetch_certificates(
        &self,
        request: anemo::Request<FetchCertificatesRequest>,
    ) -> Result<anemo::Response<FetchCertificatesResponse>, anemo::rpc::Status> {
        self.controller
            .load_full()
            .ok_or_else(|| Status::internal("Service not ready"))?
            .fetch_certificates(request)
            .await
    }

    async fn get_payload_availability(
        &self,
        request: anemo::Request<PayloadAvailabilityRequest>,
    ) -> Result<anemo::Response<PayloadAvailabilityResponse>, anemo::rpc::Status> {
        self.controller
            .load_full()
            .ok_or_else(|| Status::internal("Service not ready"))?
            .get_payload_availability(request)
            .await
    }
}

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
pub struct WorkerReceiverHandler {
    pub controller: Arc<ArcSwapOption<WorkerReceiverController>>,
}

impl WorkerReceiverHandler {
    pub fn new(controller: Arc<ArcSwapOption<WorkerReceiverController>>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl WorkerToPrimary for WorkerReceiverHandler {
    async fn report_our_batch(
        &self,
        request: anemo::Request<WorkerOurBatchMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        self.controller
            .load_full()
            .ok_or_else(|| Status::internal("Service not ready"))?
            .report_our_batch(request)
            .await
    }

    async fn report_others_batch(
        &self,
        request: anemo::Request<WorkerOthersBatchMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        self.controller
            .load_full()
            .ok_or_else(|| Status::internal("Service not ready"))?
            .report_others_batch(request)
            .await
    }

    async fn worker_info(
        &self,
        _request: anemo::Request<()>,
    ) -> Result<anemo::Response<WorkerInfoResponse>, anemo::rpc::Status> {
        self.controller
            .load_full()
            .ok_or_else(|| Status::internal("Service not ready"))?
            .worker_info(_request)
            .await
    }
}
