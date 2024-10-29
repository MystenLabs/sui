// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use mysten_network::config::Config;
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::time::Duration;
use sui_network::{api::ValidatorClient, tonic};
use sui_types::base_types::AuthorityName;
use sui_types::committee::CommitteeWithNetworkMetadata;
use sui_types::crypto::NetworkPublicKey;
use sui_types::messages_checkpoint::{
    CheckpointRequest, CheckpointRequestV2, CheckpointResponse, CheckpointResponseV2,
};
use sui_types::multiaddr::Multiaddr;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::{
    error::{SuiError, SuiResult},
    transaction::*,
};

use crate::authority_client::tonic::IntoRequest;
use sui_network::tonic::metadata::KeyAndValueRef;
use sui_network::tonic::transport::Channel;
use sui_types::messages_grpc::{
    HandleCertificateRequestV3, HandleCertificateResponseV2, HandleCertificateResponseV3,
    HandleSoftBundleCertificatesRequestV3, HandleSoftBundleCertificatesResponseV3,
    HandleTransactionResponse, ObjectInfoRequest, ObjectInfoResponse, SystemStateRequest,
    TransactionInfoRequest, TransactionInfoResponse,
};

#[async_trait]
pub trait AuthorityAPI {
    /// Initiate a new transaction to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        transaction: Transaction,
        client_addr: Option<SocketAddr>,
    ) -> Result<HandleTransactionResponse, SuiError>;

    /// Execute a certificate.
    async fn handle_certificate_v2(
        &self,
        certificate: CertifiedTransaction,
        client_addr: Option<SocketAddr>,
    ) -> Result<HandleCertificateResponseV2, SuiError>;

    /// Execute a certificate.
    async fn handle_certificate_v3(
        &self,
        request: HandleCertificateRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<HandleCertificateResponseV3, SuiError>;

    /// Execute a Soft Bundle with multiple certificates.
    async fn handle_soft_bundle_certificates_v3(
        &self,
        request: HandleSoftBundleCertificatesRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<HandleSoftBundleCertificatesResponseV3, SuiError>;

    /// Handle Object information requests for this account.
    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError>;

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError>;

    async fn handle_checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError>;

    async fn handle_checkpoint_v2(
        &self,
        request: CheckpointRequestV2,
    ) -> Result<CheckpointResponseV2, SuiError>;

    // This API is exclusively used by the benchmark code.
    // Hence it's OK to return a fixed system state type.
    async fn handle_system_state_object(
        &self,
        request: SystemStateRequest,
    ) -> Result<SuiSystemState, SuiError>;
}

#[derive(Clone)]
pub struct NetworkAuthorityClient {
    client: SuiResult<ValidatorClient<Channel>>,
}

impl NetworkAuthorityClient {
    pub async fn connect(
        address: &Multiaddr,
        tls_target: Option<NetworkPublicKey>,
    ) -> anyhow::Result<Self> {
        let tls_config = tls_target.map(|tls_target| {
            sui_tls::create_rustls_client_config(
                tls_target,
                sui_tls::SUI_VALIDATOR_SERVER_NAME.to_string(),
                None,
            )
        });
        let channel = mysten_network::client::connect(address, tls_config)
            .await
            .map_err(|err| anyhow!(err.to_string()))?;
        Ok(Self::new(channel))
    }

    pub fn connect_lazy(address: &Multiaddr, tls_target: Option<NetworkPublicKey>) -> Self {
        let tls_config = tls_target.map(|tls_target| {
            sui_tls::create_rustls_client_config(
                tls_target,
                sui_tls::SUI_VALIDATOR_SERVER_NAME.to_string(),
                None,
            )
        });
        let client: SuiResult<_> = mysten_network::client::connect_lazy(address, tls_config)
            .map(ValidatorClient::new)
            .map_err(|err| err.to_string().into());
        Self { client }
    }

    pub fn new(channel: Channel) -> Self {
        Self {
            client: Ok(ValidatorClient::new(channel)),
        }
    }

    fn new_lazy(client: SuiResult<Channel>) -> Self {
        Self {
            client: client.map(ValidatorClient::new),
        }
    }

    fn client(&self) -> SuiResult<ValidatorClient<Channel>> {
        self.client.clone()
    }
}

#[async_trait]
impl AuthorityAPI for NetworkAuthorityClient {
    /// Initiate a new transfer to a Sui or Primary account.
    async fn handle_transaction(
        &self,
        transaction: Transaction,
        client_addr: Option<SocketAddr>,
    ) -> Result<HandleTransactionResponse, SuiError> {
        let mut request = transaction.into_request();
        insert_metadata(&mut request, client_addr);

        self.client()?
            .transaction(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    /// Execute a certificate.
    async fn handle_certificate_v2(
        &self,
        certificate: CertifiedTransaction,
        client_addr: Option<SocketAddr>,
    ) -> Result<HandleCertificateResponseV2, SuiError> {
        let mut request = certificate.into_request();
        insert_metadata(&mut request, client_addr);

        let response = self
            .client()?
            .handle_certificate_v2(request)
            .await
            .map(tonic::Response::into_inner);

        response.map_err(Into::into)
    }

    async fn handle_certificate_v3(
        &self,
        request: HandleCertificateRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<HandleCertificateResponseV3, SuiError> {
        let mut request = request.into_request();
        insert_metadata(&mut request, client_addr);

        let response = self
            .client()?
            .handle_certificate_v3(request)
            .await
            .map(tonic::Response::into_inner);

        response.map_err(Into::into)
    }

    async fn handle_soft_bundle_certificates_v3(
        &self,
        request: HandleSoftBundleCertificatesRequestV3,
        client_addr: Option<SocketAddr>,
    ) -> Result<HandleSoftBundleCertificatesResponseV3, SuiError> {
        let mut request = request.into_request();
        insert_metadata(&mut request, client_addr);

        let response = self
            .client()?
            .handle_soft_bundle_certificates_v3(request)
            .await
            .map(tonic::Response::into_inner);

        response.map_err(Into::into)
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        self.client()?
            .object_info(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        self.client()?
            .transaction_info(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    /// Handle Object information requests for this account.
    async fn handle_checkpoint(
        &self,
        request: CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        self.client()?
            .checkpoint(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    /// Handle Object information requests for this account.
    async fn handle_checkpoint_v2(
        &self,
        request: CheckpointRequestV2,
    ) -> Result<CheckpointResponseV2, SuiError> {
        self.client()?
            .checkpoint_v2(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }

    async fn handle_system_state_object(
        &self,
        request: SystemStateRequest,
    ) -> Result<SuiSystemState, SuiError> {
        self.client()?
            .get_system_state_object(request)
            .await
            .map(tonic::Response::into_inner)
            .map_err(Into::into)
    }
}

pub fn make_network_authority_clients_with_network_config(
    committee: &CommitteeWithNetworkMetadata,
    network_config: &Config,
) -> BTreeMap<AuthorityName, NetworkAuthorityClient> {
    let mut authority_clients = BTreeMap::new();
    for (name, (_state, network_metadata)) in committee.validators() {
        let address = network_metadata.network_address.clone();
        let address = address.rewrite_udp_to_tcp();
        // TODO: Enable TLS on this interface with below config, once support is rolled out to validators.
        // let tls_config = network_metadata.network_public_key.as_ref().map(|key| {
        //     sui_tls::create_rustls_client_config(
        //         key.clone(),
        //         sui_tls::SUI_VALIDATOR_SERVER_NAME.to_string(),
        //         None,
        //     )
        // });
        // TODO: Change below code to generate a SuiError if no valid TLS config is available.
        let maybe_channel = network_config.connect_lazy(&address, None).map_err(|e| {
            tracing::error!(
                address = %address,
                name = %name,
                "unable to create authority client: {e}"
            );
            e.to_string().into()
        });
        let client = NetworkAuthorityClient::new_lazy(maybe_channel);
        authority_clients.insert(*name, client);
    }
    authority_clients
}

pub fn make_authority_clients_with_timeout_config(
    committee: &CommitteeWithNetworkMetadata,
    connect_timeout: Duration,
    request_timeout: Duration,
) -> BTreeMap<AuthorityName, NetworkAuthorityClient> {
    let mut network_config = mysten_network::config::Config::new();
    network_config.connect_timeout = Some(connect_timeout);
    network_config.request_timeout = Some(request_timeout);
    make_network_authority_clients_with_network_config(committee, &network_config)
}

fn insert_metadata<T>(request: &mut tonic::Request<T>, client_addr: Option<SocketAddr>) {
    if let Some(client_addr) = client_addr {
        let mut metadata = tonic::metadata::MetadataMap::new();
        metadata.insert("x-forwarded-for", client_addr.to_string().parse().unwrap());
        metadata
            .iter()
            .for_each(|key_and_value| match key_and_value {
                KeyAndValueRef::Ascii(key, value) => {
                    request.metadata_mut().insert(key, value.clone());
                }
                KeyAndValueRef::Binary(key, value) => {
                    request.metadata_mut().insert_bin(key, value.clone());
                }
            });
    }
}
