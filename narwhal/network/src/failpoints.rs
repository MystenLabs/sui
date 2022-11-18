use std::collections::HashMap;

use anemo_tower::callback::{MakeCallbackHandler, ResponseHandler};
use fail::fail_point;
use tracing::warn;

/// Initializes network failpoints
pub fn initialise_network_failpoints() {
    let mut failpoints: HashMap<String, String> = HashMap::new();
    failpoints.insert(String::from("rpc-delay"), String::from("1%delay(10000)"));

    if fail::has_failpoints() {
        warn!("Failpoints are enabled");
        for (point, actions) in failpoints {
            fail::cfg(point, &actions).expect("failed to set actions for failpoints");
        }
    } else {
        warn!("Failpoints are not enabled");
    }
}

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
