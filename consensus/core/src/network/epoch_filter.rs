// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anemo::{rpc::Status, Request, Response};
use anemo_tower::auth::AuthorizeRequest;
use bytes::Bytes;

/// The epoch header attached to all network requests.
pub const EPOCH_HEADER_KEY: &str = "epoch";

#[derive(Clone, Debug)]
pub struct AllowedEpoch {
    allowed_epoch: String,
}

impl AllowedEpoch {
    pub fn new(epoch: String) -> Self {
        Self {
            allowed_epoch: epoch,
        }
    }
}

impl AuthorizeRequest for AllowedEpoch {
    fn authorize(&self, request: &mut Request<Bytes>) -> Result<(), Response<Bytes>> {
        use anemo::types::response::{IntoResponse, StatusCode};

        let epoch = request.headers().get(EPOCH_HEADER_KEY).ok_or_else(|| {
            Status::new_with_message(StatusCode::BadRequest, "missing epoch header").into_response()
        })?;

        if self.allowed_epoch == *epoch {
            Ok(())
        } else {
            Err(Status::new_with_message(
                StatusCode::BadRequest,
                format!(
                    "request from epoch {:?} does not match current epoch {:?}",
                    epoch, self.allowed_epoch
                ),
            )
            .into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use anemo::{types::response::StatusCode, Request, Response};
    use anemo_tower::auth::RequireAuthorizationLayer;
    use bytes::Bytes;
    use tower::{BoxError, Service, ServiceBuilder, ServiceExt};

    use super::*;

    #[tokio::test]
    async fn authorize_request_by_epoch() {
        // Authorize requests that have a particular header set
        let auth_layer = RequireAuthorizationLayer::new(AllowedEpoch::new("3".to_string()));

        let mut svc = ServiceBuilder::new().layer(auth_layer).service_fn(echo);

        // Unable to query requesters PeerId
        let response = svc
            .ready()
            .await
            .unwrap()
            .call(Request::new(Bytes::from("foobar")))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BadRequest);

        // Previous Epoch Request
        let response = svc
            .ready()
            .await
            .unwrap()
            .call(Request::new(Bytes::from("foobar")).with_header("epoch", "2"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BadRequest);

        // Allowed Epoch Request
        let response = svc
            .ready()
            .await
            .unwrap()
            .call(Request::new(Bytes::from("foobar")).with_header("epoch", "3"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::Success);
        assert_eq!(response.inner(), "foobar");
    }

    async fn echo(req: Request<Bytes>) -> Result<Response<Bytes>, BoxError> {
        Ok(Response::new(req.into_body()))
    }
}
