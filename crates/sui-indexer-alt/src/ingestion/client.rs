// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use backoff::ExponentialBackoff;
use reqwest::{Client, StatusCode};
use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error};
use url::Url;

use crate::ingestion::error::{Error, Result};
use crate::metrics::IndexerMetrics;

/// Wait at most this long between retries for transient errors.
const MAX_TRANSIENT_RETRY_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub(crate) struct IngestionClient {
    url: Url,
    client: Client,
    /// Wrap the metrics in an `Arc` to keep copies of the client cheap.
    metrics: Arc<IndexerMetrics>,
}

impl IngestionClient {
    pub(crate) fn new(url: Url, metrics: Arc<IndexerMetrics>) -> Result<Self> {
        Ok(Self {
            url,
            client: Client::builder().build()?,
            metrics,
        })
    }

    /// Fetch a checkpoint from the remote store. Repeatedly retries transient errors with an
    /// exponential backoff (up to [MAX_RETRY_INTERVAL]), but will immediately return on:
    ///
    /// - non-transient errors, which include all client errors, except timeouts and rate limiting.
    /// - cancellation of the supplied `cancel` token.
    ///
    /// Transient errors include:
    ///
    /// - failures to issue a request, (network errors, redirect issues, etc)
    /// - request timeouts,
    /// - rate limiting,
    /// - server errors (5xx),
    /// - issues getting a full response and deserializing it as [CheckpointData].
    pub(crate) async fn fetch(
        &self,
        checkpoint: u64,
        cancel: &CancellationToken,
    ) -> Result<Arc<CheckpointData>> {
        // SAFETY: The path being joined is statically known to be valid.
        let url = self
            .url
            .join(&format!("/{checkpoint}.chk"))
            .expect("Unexpected invalid URL");

        let request = move || {
            let url = url.clone();
            async move {
                use backoff::Error as BE;
                if cancel.is_cancelled() {
                    return Err(BE::permanent(Error::Cancelled));
                }

                let response = self.client.get(url).send().await.map_err(|e| {
                    self.metrics
                        .inc_retry(checkpoint, "request", Error::ReqwestError(e))
                })?;

                match response.status() {
                    code if code.is_success() => {
                        // Failure to extract all the bytes from the payload, or to deserialize the
                        // checkpoint from them is considered a transient error -- the store being
                        // fetched from needs to be corrected, and ingestion will keep retrying it
                        // until it is.
                        let bytes = response.bytes().await.map_err(|e| {
                            self.metrics
                                .inc_retry(checkpoint, "bytes", Error::ReqwestError(e))
                        })?;

                        self.metrics.total_ingested_bytes.inc_by(bytes.len() as u64);
                        let data: CheckpointData = Blob::from_bytes(&bytes).map_err(|e| {
                            self.metrics.inc_retry(
                                checkpoint,
                                "deserialization",
                                Error::DeserializationError(checkpoint, e),
                            )
                        })?;

                        Ok(data)
                    }

                    // Treat 404s as a special case so we can match on this error type.
                    code @ StatusCode::NOT_FOUND => {
                        debug!(checkpoint, %code, "Checkpoint not found");
                        Err(BE::permanent(Error::NotFound(checkpoint)))
                    }

                    // Timeouts are a client error but they are usually transient.
                    code @ StatusCode::REQUEST_TIMEOUT => Err(self.metrics.inc_retry(
                        checkpoint,
                        "timeout",
                        Error::HttpError(checkpoint, code),
                    )),

                    // Rate limiting is also a client error, but the backoff will eventually widen the
                    // interval appropriately.
                    code @ StatusCode::TOO_MANY_REQUESTS => Err(self.metrics.inc_retry(
                        checkpoint,
                        "too_many_requests",
                        Error::HttpError(checkpoint, code),
                    )),

                    // Assume that if the server is facing difficulties, it will recover eventually.
                    code if code.is_server_error() => Err(self.metrics.inc_retry(
                        checkpoint,
                        "server_error",
                        Error::HttpError(checkpoint, code),
                    )),

                    // For everything else, assume it's a permanent error and don't retry.
                    code => {
                        error!(checkpoint, %code, "Permanent error, giving up!");
                        Err(BE::permanent(Error::HttpError(checkpoint, code)))
                    }
                }
            }
        };

        // Keep backing off until we are waiting for the max interval, but don't give up.
        let backoff = ExponentialBackoff {
            max_interval: MAX_TRANSIENT_RETRY_INTERVAL,
            max_elapsed_time: None,
            ..Default::default()
        };

        let guard = self.metrics.ingested_checkpoint_latency.start_timer();
        let data = backoff::future::retry(backoff, request).await?;
        let elapsed = guard.stop_and_record();

        debug!(
            checkpoint,
            elapsed_ms = elapsed * 1000.0,
            "Fetched checkpoint"
        );

        self.metrics.total_ingested_checkpoints.inc();

        self.metrics
            .total_ingested_transactions
            .inc_by(data.transactions.len() as u64);

        self.metrics.total_ingested_events.inc_by(
            data.transactions
                .iter()
                .map(|tx| tx.events.as_ref().map_or(0, |evs| evs.data.len()) as u64)
                .sum(),
        );

        self.metrics.total_ingested_inputs.inc_by(
            data.transactions
                .iter()
                .map(|tx| tx.input_objects.len() as u64)
                .sum(),
        );

        self.metrics.total_ingested_outputs.inc_by(
            data.transactions
                .iter()
                .map(|tx| tx.output_objects.len() as u64)
                .sum(),
        );

        Ok(Arc::new(data))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;

    use rand::{rngs::StdRng, SeedableRng};
    use sui_storage::blob::BlobEncoding;
    use sui_types::{
        crypto::KeypairTraits,
        gas::GasCostSummary,
        messages_checkpoint::{
            CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary,
            SignedCheckpointSummary,
        },
        supported_protocol_versions::ProtocolConfig,
        utils::make_committee_key,
    };
    use wiremock::{
        matchers::{method, path_regex},
        Mock, MockServer, Request, Respond, ResponseTemplate,
    };

    use crate::metrics::tests::test_metrics;

    use super::*;

    const RNG_SEED: [u8; 32] = [
        21, 23, 199, 200, 234, 250, 252, 178, 94, 15, 202, 178, 62, 186, 88, 137, 233, 192, 130,
        157, 179, 179, 65, 9, 31, 249, 221, 123, 225, 112, 199, 247,
    ];

    pub(crate) async fn respond_with(server: &MockServer, response: impl Respond + 'static) {
        Mock::given(method("GET"))
            .and(path_regex(r"/\d+.chk"))
            .respond_with(response)
            .mount(server)
            .await;
    }

    pub(crate) fn status(code: StatusCode) -> ResponseTemplate {
        ResponseTemplate::new(code.as_u16())
    }

    pub(crate) fn test_checkpoint_data(cp: u64) -> Vec<u8> {
        let mut rng = StdRng::from_seed(RNG_SEED);
        let (keys, committee) = make_committee_key(&mut rng);
        let contents = CheckpointContents::new_with_digests_only_for_tests(vec![]);
        let summary = CheckpointSummary::new(
            &ProtocolConfig::get_for_max_version_UNSAFE(),
            0,
            cp,
            0,
            &contents,
            None,
            GasCostSummary::default(),
            None,
            0,
            Vec::new(),
        );

        let sign_infos: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public().into();
                SignedCheckpointSummary::sign(committee.epoch, &summary, k, name)
            })
            .collect();

        let checkpoint_data = CheckpointData {
            checkpoint_summary: CertifiedCheckpointSummary::new(summary, sign_infos, &committee)
                .unwrap(),
            checkpoint_contents: contents,
            transactions: vec![],
        };

        Blob::encode(&checkpoint_data, BlobEncoding::Bcs)
            .unwrap()
            .to_bytes()
    }

    fn test_client(uri: String) -> IngestionClient {
        IngestionClient::new(Url::parse(&uri).unwrap(), Arc::new(test_metrics())).unwrap()
    }

    #[tokio::test]
    async fn fail_on_not_found() {
        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::NOT_FOUND)).await;

        let client = test_client(server.uri());
        let error = client
            .fetch(42, &CancellationToken::new())
            .await
            .unwrap_err();

        assert!(matches!(error, Error::NotFound(42)));
    }

    #[tokio::test]
    async fn fail_on_client_error() {
        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::IM_A_TEAPOT)).await;

        let client = test_client(server.uri());
        let error = client
            .fetch(42, &CancellationToken::new())
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            Error::HttpError(42, StatusCode::IM_A_TEAPOT)
        ));
    }

    /// Even if the server is repeatedly returning transient errors, it is possible to cancel the
    /// fetch request via its cancellation token.
    #[tokio::test]
    async fn fail_on_cancel() {
        let cancel = CancellationToken::new();
        let server = MockServer::start().await;

        // This mock server repeatedly returns internal server errors, but will also send a
        // cancellation with the second request (this is a bit of a contrived test set-up).
        let times: Mutex<u64> = Mutex::new(0);
        let server_cancel = cancel.clone();
        respond_with(&server, move |_: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;

            if *times > 2 {
                server_cancel.cancel();
            }

            status(StatusCode::INTERNAL_SERVER_ERROR)
        })
        .await;

        let client = test_client(server.uri());
        let error = client.fetch(42, &cancel.clone()).await.unwrap_err();

        assert!(matches!(error, Error::Cancelled));
    }

    /// Assume that failures to send the request to the remote store are due to temporary
    /// connectivity issues, and retry them.
    #[tokio::test]
    async fn retry_on_request_error() {
        let server = MockServer::start().await;

        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |r: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            match (*times, r.url.path()) {
                // The first request will trigger a redirect to 0.chk no matter what the original
                // request was for -- triggering a request error.
                (1, _) => status(StatusCode::MOVED_PERMANENTLY).append_header("Location", "/0.chk"),

                // Set-up checkpoint 0 as an infinite redirect loop.
                (_, "/0.chk") => {
                    status(StatusCode::MOVED_PERMANENTLY).append_header("Location", r.url.as_str())
                }

                // Subsequently, requests will fail with a permanent error, this is what we expect
                // to see.
                _ => status(StatusCode::IM_A_TEAPOT),
            }
        })
        .await;

        let client = test_client(server.uri());
        let error = client
            .fetch(42, &CancellationToken::new())
            .await
            .unwrap_err();

        assert!(
            matches!(error, Error::HttpError(42, StatusCode::IM_A_TEAPOT),),
            "{error}"
        );
    }

    /// Assume that certain errors will recover by themselves, and keep retrying with an
    /// exponential back-off. These errors include: 5xx (server) errors, 408 (timeout), and 429
    /// (rate limiting).
    #[tokio::test]
    async fn retry_on_transient_server_error() {
        let server = MockServer::start().await;
        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |_: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            status(match *times {
                1 => StatusCode::INTERNAL_SERVER_ERROR,
                2 => StatusCode::REQUEST_TIMEOUT,
                3 => StatusCode::TOO_MANY_REQUESTS,
                _ => StatusCode::IM_A_TEAPOT,
            })
        })
        .await;

        let client = test_client(server.uri());
        let error = client
            .fetch(42, &CancellationToken::new())
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            Error::HttpError(42, StatusCode::IM_A_TEAPOT)
        ));
    }

    /// Treat deserialization failure as another kind of transient error -- all checkpoint data
    /// that is fetched should be valid (deserializable as a `CheckpointData`).
    #[tokio::test]
    async fn retry_on_deserialization_error() {
        let server = MockServer::start().await;
        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |_: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            if *times < 3 {
                status(StatusCode::OK).set_body_bytes(vec![])
            } else {
                status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42))
            }
        })
        .await;

        let client = test_client(server.uri());
        let checkpoint = client.fetch(42, &CancellationToken::new()).await.unwrap();
        assert_eq!(42, checkpoint.checkpoint_summary.sequence_number)
    }
}
