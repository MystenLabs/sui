// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! DNS-resolution metering for the checkpoint store's HTTP client.
//!
//! object_store's HTTP store resolves DNS on every new connection (via an uncached resolver) and
//! exposes no hook to observe how often it does so. To surface that -- the "one DNS lookup per
//! checkpoint when connections aren't reused" behaviour this store is prone to -- this module
//! provides a [`MeteredHttpConnector`] that rebuilds the reqwest client with a [`MeteredDnsResolver`]
//! wrapping object_store's address-shuffling resolution, incrementing
//! [`IngestionMetrics::total_dns_resolutions`](crate::metrics::IngestionMetrics) on every lookup.

use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::Duration;

use object_store::ClientOptions;
use object_store::client::HttpClient;
use object_store::client::HttpConnector;
use prometheus::IntCounter;
use rand::seq::SliceRandom;
use reqwest::dns::Addrs;
use reqwest::dns::Name;
use reqwest::dns::Resolve;
use reqwest::dns::Resolving;

/// A [`Resolve`] that increments a counter on every resolution and delegates the actual lookup to an
/// inner resolver.
struct MeteredDnsResolver {
    dns_resolutions: IntCounter,
    inner: Arc<dyn Resolve>,
}

impl MeteredDnsResolver {
    fn new(dns_resolutions: IntCounter, inner: Arc<dyn Resolve>) -> Self {
        Self {
            dns_resolutions,
            inner,
        }
    }
}

impl Resolve for MeteredDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        self.dns_resolutions.inc();
        self.inner.resolve(name)
    }
}

/// A real resolver mirroring object_store's default `ShuffleResolver`: a blocking `getaddrinfo` run
/// off the async runtime, with the resolved addresses shuffled for load-balancing. It caches
/// nothing, so every call is a fresh lookup.
struct ShufflingResolver;

impl Resolve for ShufflingResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addrs = tokio::task::spawn_blocking(move || {
                let mut addrs: Vec<SocketAddr> = (name.as_str(), 0).to_socket_addrs()?.collect();
                addrs.shuffle(&mut rand::thread_rng());
                std::io::Result::Ok(addrs)
            })
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)??;
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// An object_store [`HttpConnector`] that builds the reqwest client with a [`MeteredDnsResolver`], so
/// every DNS lookup the checkpoint store performs is counted. object_store offers no hook to add a
/// resolver to its default client, so this rebuilds the client, re-applying the options the ingestion
/// configures: the request/connect timeouts (passed in, as `ClientOptions` does not expose them) and
/// default headers. If the ingestion starts configuring other `ClientOptions` (proxy, TLS, ...), they
/// must be re-applied here as well.
pub(crate) struct MeteredHttpConnector {
    dns_resolutions: IntCounter,
    request_timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    ca_certificate: Option<reqwest::tls::Certificate>,
    http1_only: bool,
}

impl MeteredHttpConnector {
    pub(crate) fn new(
        dns_resolutions: IntCounter,
        request_timeout: Option<Duration>,
        connect_timeout: Option<Duration>,
        ca_certificate: Option<reqwest::tls::Certificate>,
        http1_only: bool,
    ) -> Self {
        Self {
            dns_resolutions,
            request_timeout,
            connect_timeout,
            ca_certificate,
            http1_only,
        }
    }
}

impl std::fmt::Debug for MeteredHttpConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeteredHttpConnector")
            .field("request_timeout", &self.request_timeout)
            .field("connect_timeout", &self.connect_timeout)
            .field("ca_certificate", &self.ca_certificate.is_some())
            .field("http1_only", &self.http1_only)
            .finish()
    }
}

impl HttpConnector for MeteredHttpConnector {
    fn connect(&self, options: &ClientOptions) -> object_store::Result<HttpClient> {
        let resolver =
            MeteredDnsResolver::new(self.dns_resolutions.clone(), Arc::new(ShufflingResolver));
        let mut builder = reqwest::Client::builder().dns_resolver(Arc::new(resolver));

        if let Some(timeout) = self.request_timeout {
            builder = builder.timeout(timeout);
        }
        if let Some(timeout) = self.connect_timeout {
            builder = builder.connect_timeout(timeout);
        }
        if let Some(headers) = options.get_default_headers() {
            builder = builder.default_headers(headers.clone());
        }
        if let Some(cert) = &self.ca_certificate {
            builder = builder.add_root_certificate(cert.clone());
        }
        if self.http1_only {
            builder = builder.http1_only();
        }

        let client = builder.build().map_err(|e| object_store::Error::Generic {
            store: "metered-http-connector",
            source: Box::new(e),
        })?;
        Ok(HttpClient::new(client))
    }
}
