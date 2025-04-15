use crate::proto::rpc::v2beta::sleep_service_server::SleepService;
use crate::proto::rpc::v2beta::{SleepRequest, SleepResponse};
use crate::RpcService;
use prost_types::DurationError;
use std::time::Duration;

#[tonic::async_trait]
impl SleepService for RpcService {
    async fn sleep(
        &self,
        request: tonic::Request<SleepRequest>,
    ) -> std::result::Result<tonic::Response<SleepResponse>, tonic::Status> {
        let timeout = request.into_inner().timeout.unwrap_or_default();
        let duration: Result<Duration, DurationError> = timeout.try_into();
        let duration = duration.map_err(|e| tonic::Status::invalid_argument(e.to_string()))?;
        tokio::time::sleep(duration).await;
        Ok(tonic::Response::new(SleepResponse {
            msg: format!("slept for {:?}", duration),
        }))
    }
}
