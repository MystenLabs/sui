use anemo_tower::callback::{MakeCallbackHandler, ResponseHandler};
use fail::fail_point;

#[derive(Clone, Default)]
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
        // TODO: Use tokio::sleep() instead of built in sleep()/delay()
        // Warning: if this failpoint is used with the default sleep()
        // or delay() it could end up blocking the system and causing other
        // unintended effects.
        fail_point!("rpc-delay");
    }

    fn on_error<E>(self, _error: &E) {}
}
