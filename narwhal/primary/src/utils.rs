// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::WorkerId;
use std::collections::HashMap;
use types::{BatchDigest, Certificate, CertificateAPI, HeaderAPI};

// a helper method that collects all the batches from each certificate and maps
// them by the worker id.
pub fn map_certificate_batches_by_worker(
    certificates: &[Certificate],
) -> HashMap<WorkerId, Vec<BatchDigest>> {
    let mut batches_by_worker: HashMap<WorkerId, Vec<BatchDigest>> = HashMap::new();
    for certificate in certificates.iter() {
        for (batch_id, (worker_id, _)) in certificate.header().payload() {
            batches_by_worker
                .entry(*worker_id)
                .or_default()
                .push(*batch_id);
        }
    }

    batches_by_worker
}
