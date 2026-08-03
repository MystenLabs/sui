// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::endpoint_manager::AddressSource;
use prometheus::{
    IntGauge, IntGaugeVec, Registry, register_int_gauge_vec_with_registry,
    register_int_gauge_with_registry,
};
use std::sync::Arc;
use tap::Pipe;

#[derive(Clone)]
pub(super) struct Metrics(Option<Arc<Inner>>);

impl std::fmt::Debug for Metrics {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        fmt.debug_struct("Metrics").finish()
    }
}

impl Metrics {
    pub fn enabled(registry: &Registry) -> Self {
        Metrics(Some(Inner::new(registry)))
    }

    pub fn disabled() -> Self {
        Metrics(None)
    }

    /// Sets the gauge of distinct peers known to have an external address.
    pub fn set_num_peers_with_external_address(&self, value: i64) {
        if let Some(inner) = &self.0 {
            inner.num_peers_with_external_address.set(value);
        }
    }

    /// Records the currently-active P2P address source for a trusted peer as a
    /// single per-peer gauge whose value is the source's `metric_code` (0 = no
    /// address installed / all sources cleared).
    pub fn set_active_p2p_address_source(&self, peer_id: &str, active: Option<AddressSource>) {
        if let Some(inner) = &self.0 {
            let code = active.map_or(0, AddressSource::metric_code);
            inner
                .active_p2p_address_source
                .with_label_values(&[peer_id])
                .set(code);
        }
    }
}

struct Inner {
    num_peers_with_external_address: IntGauge,
    active_p2p_address_source: IntGaugeVec,
}

impl Inner {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Self {
            num_peers_with_external_address: register_int_gauge_with_registry!(
                "num_peers_with_external_address",
                "Number of peers with an external address configured for discovery",
                registry
            )
            .unwrap(),
            active_p2p_address_source: register_int_gauge_vec_with_registry!(
                "discovery_active_p2p_address_source",
                "Active P2P address source per trusted peer, encoded as the gauge value: \
                 0=none (no address installed), 1=admin, 2=config, 3=discovery, 4=seed, \
                 5=chain (highest to lowest priority). One series per peer; `peer_id` is the \
                 full hex peer id.",
                &["peer_id"],
                registry
            )
            .unwrap(),
        }
        .pipe(Arc::new)
    }
}
