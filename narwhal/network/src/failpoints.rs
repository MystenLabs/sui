use anemo_tower::callback::{MakeCallbackHandler, ResponseHandler};
use fail::fail_point;

#[derive(Clone)]
pub struct FailpointsMakeCallbackHandler {}

impl FailpointsMakeCallbackHandler {
    pub fn new() -> Self {
        Self {}
    }
}

impl MakeCallbackHandler for FailpointsMakeCallbackHandler {
    type Handler = FailpointsResponseHandler;

    fn make_handler(&self, _request: &anemo::Request<bytes::Bytes>) -> Self::Handler {
        FailpointsResponseHandler {}
    }
}

pub struct FailpointsResponseHandler {}

impl ResponseHandler for FailpointsResponseHandler {
    fn on_response(self, _response: &anemo::Response<bytes::Bytes>) {
        fail_point!("rpc-delay");
    }

    fn on_error<E>(self, _error: &E) {}
}
