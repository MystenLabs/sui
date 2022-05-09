// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crypto::{traits::VerifyingKey, Hash};
use dag::node_dag::NodeDag;
use std::collections::BTreeMap;
use tokio::{sync::mpsc::Receiver, task::JoinHandle};
use tracing::instrument;
use types::{error::DagResult, Certificate, CertificateDigest, Round};

/// Dag represents the pure dag that is constructed
/// by the certificate of each round without any
/// consensus running on top of it.
#[derive(Debug)]
pub struct Dag<PublicKey: VerifyingKey> {
    /// Receives new certificates from the primary. The primary should send us new certificates only
    /// if it already sent us its whole history.
    rx_primary: Receiver<Certificate<PublicKey>>,

    /// The Virtual DAG data structure, which lets us track certificates in a memory-conscious way
    dag: NodeDag<Certificate<PublicKey>>,

    /// Secondary index: An authority-aware map of the DAG's veertex Certificates
    vertices: BTreeMap<(Round, PublicKey), CertificateDigest>,
}

impl<PublicKey: VerifyingKey> Dag<PublicKey> {
    pub fn spawn(rx_primary: Receiver<Certificate<PublicKey>>) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self {
                rx_primary,
                dag: NodeDag::new(),
                vertices: BTreeMap::new(),
            }
            .run()
            .await
        })
    }

    async fn run(&mut self) {
        // at the moment just receive the certificate and throw away
        while let Some(certificate) = self.rx_primary.recv().await {
            // The Core (process_certificate) guarantees the certificate
            // has gone through causal completion => this is ready to be inserted
            let _ = self.insert(certificate);
        }
    }

    #[instrument(level = "debug", err)]
    pub fn insert(&mut self, certificate: Certificate<PublicKey>) -> DagResult<()> {
        let digest = certificate.digest();
        let round = certificate.round();
        let origin = certificate.origin();

        // This fails if the validation of the certificate is incomplete
        self.dag.try_insert(certificate)?;
        self.vertices.insert((round, origin), digest);
        Ok(())
    }
}
