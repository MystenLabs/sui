// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context::Context;
use consensus_config::{AuthorityIndex, NetworkKeyPair};
use tokio_rustls::rustls::ClientConfig;

pub(crate) fn create_rustls_client_config(
    context: &Context,
    network_keypair: NetworkKeyPair,
    target: AuthorityIndex,
) -> ClientConfig {
    let target_public_key = context
        .committee
        .authority(target)
        .network_key
        .clone()
        .into_inner();
    let self_signed_cert = sui_tls::SelfSignedCertificate::new(
        network_keypair.private_key().into_inner(),
        &certificate_server_name(context),
    );
    let tls_cert = self_signed_cert.rustls_certificate();
    let tls_private_key = self_signed_cert.rustls_private_key();
    let mut tls_config =
        sui_tls::ServerCertVerifier::new(target_public_key, certificate_server_name(context))
            .rustls_client_config(vec![tls_cert], tls_private_key)
            .unwrap_or_else(|e| panic!("Failed to create TLS client config: {:?}", e));
    // ServerCertVerifier sets alpn for completeness, but alpn cannot be predefined when
    // using HttpsConnector from hyper-rustls, as in TonicManager.
    tls_config.alpn_protocols = vec![];
    tls_config
}

pub fn certificate_server_name(context: &Context) -> String {
    format!("consensus_epoch_{}", context.committee.epoch())
}
