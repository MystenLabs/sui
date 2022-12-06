use arc_swap::ArcSwap;
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
pub use crate::handlers::primary::TraitPrimaryReceiverController;
pub use crate::handlers::primary::UnimplementedPrimaryReceiverController;
pub use crate::handlers::worker::TraitWorkerReceiverController;
pub use crate::handlers::worker::UnimplementedWorkerReceiverController;
pub use crate::handlers::worker::WorkerReceiverController;

mod primary;
mod worker;

/// Defines how the network receiver handles incoming primary messages.
#[derive(Clone)]
pub struct PrimaryReceiverHandler<T: TraitPrimaryReceiverController> {
    pub controller: Arc<ArcSwap<T>>,
}

#[allow(clippy::result_large_err)]
impl<T: TraitPrimaryReceiverController> PrimaryReceiverHandler<T> {
    pub fn new(controller: Arc<ArcSwap<T>>) -> Self {
        PrimaryReceiverHandler { controller }
    }
}

#[async_trait]
impl<T: TraitPrimaryReceiverController> PrimaryToPrimary for PrimaryReceiverHandler<T> {
    async fn send_message(
        &self,
        request: anemo::Request<PrimaryMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        self.controller.load().send_message(request).await
    }

    async fn request_vote(
        &self,
        request: anemo::Request<RequestVoteRequest>,
    ) -> Result<anemo::Response<RequestVoteResponse>, anemo::rpc::Status> {
        self.controller.load().request_vote(request).await
    }

    async fn get_certificates(
        &self,
        request: anemo::Request<GetCertificatesRequest>,
    ) -> Result<anemo::Response<GetCertificatesResponse>, anemo::rpc::Status> {
        self.controller.load().get_certificates(request).await
    }

    #[instrument(level = "debug", skip_all, peer = ?request.peer_id())]
    async fn fetch_certificates(
        &self,
        request: anemo::Request<FetchCertificatesRequest>,
    ) -> Result<anemo::Response<FetchCertificatesResponse>, anemo::rpc::Status> {
        self.controller.load().fetch_certificates(request).await
    }

    async fn get_payload_availability(
        &self,
        request: anemo::Request<PayloadAvailabilityRequest>,
    ) -> Result<anemo::Response<PayloadAvailabilityResponse>, anemo::rpc::Status> {
        self.controller
            .load()
            .get_payload_availability(request)
            .await
    }
}

/// Defines how the network receiver handles incoming workers messages.
#[derive(Clone)]
pub struct WorkerReceiverHandler<T: TraitWorkerReceiverController> {
    pub controller: Arc<ArcSwap<T>>,
}

impl<T: TraitWorkerReceiverController> WorkerReceiverHandler<T> {
    pub fn new(controller: Arc<ArcSwap<T>>) -> Self {
        Self { controller }
    }
}

#[async_trait]
impl<T: TraitWorkerReceiverController> WorkerToPrimary for WorkerReceiverHandler<T> {
    async fn report_our_batch(
        &self,
        request: anemo::Request<WorkerOurBatchMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        self.controller.load().report_our_batch(request).await
    }

    async fn report_others_batch(
        &self,
        request: anemo::Request<WorkerOthersBatchMessage>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        self.controller.load().report_others_batch(request).await
    }

    async fn worker_info(
        &self,
        _request: anemo::Request<()>,
    ) -> Result<anemo::Response<WorkerInfoResponse>, anemo::rpc::Status> {
        self.controller.load().worker_info(_request).await
    }
}
