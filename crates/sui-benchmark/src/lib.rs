// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::bail;
use async_trait::async_trait;
use fullnode_reconfig_observer::FullNodeReconfigObserver;
use prometheus::Registry;
use std::{collections::BTreeMap, sync::Arc};
use sui_config::genesis::Genesis;
use sui_config::NetworkConfig;
use sui_core::{
    authority_aggregator::AuthorityAggregatorBuilder,
    authority_client::NetworkAuthorityClient,
    quorum_driver::{
        QuorumDriver, QuorumDriverHandler, QuorumDriverHandlerBuilder, QuorumDriverMetrics,
    },
};
use sui_json_rpc_types::{SuiCertifiedTransaction, SuiObjectRead, SuiTransactionEffects};
use sui_sdk::SuiClient;
use sui_types::{
    base_types::ObjectID,
    committee::{Committee, EpochId},
    messages::{CertifiedTransactionEffects, QuorumDriverResponse, Transaction},
    object::{Object, ObjectRead},
};
use sui_types::{
    base_types::ObjectRef, crypto::AuthorityStrongQuorumSignInfo,
    messages::ExecuteTransactionRequestType, object::Owner,
};

pub mod drivers;
pub mod fullnode_reconfig_observer;
pub mod util;
pub mod workloads;

/// A wrapper on execution results to accommodate different types of
/// responses from LocalValidatorAggregatorProxy and FullNodeProxy
#[allow(clippy::large_enum_variant)]
pub enum ExecutionEffects {
    CertifiedTransactionEffects(CertifiedTransactionEffects),
    SuiTransactionEffects(SuiTransactionEffects),
}

impl ExecutionEffects {
    pub fn mutated(&self) -> Vec<(ObjectRef, Owner)> {
        match self {
            ExecutionEffects::CertifiedTransactionEffects(certified_effects) => {
                certified_effects.data().mutated.clone()
            }
            ExecutionEffects::SuiTransactionEffects(sui_tx_effects) => sui_tx_effects
                .mutated
                .clone()
                .into_iter()
                .map(|refe| (refe.reference.to_object_ref(), refe.owner))
                .collect(),
        }
    }

    pub fn created(&self) -> Vec<(ObjectRef, Owner)> {
        match self {
            ExecutionEffects::CertifiedTransactionEffects(certified_effects) => {
                certified_effects.data().created.clone()
            }
            ExecutionEffects::SuiTransactionEffects(sui_tx_effects) => sui_tx_effects
                .created
                .clone()
                .into_iter()
                .map(|refe| (refe.reference.to_object_ref(), refe.owner))
                .collect(),
        }
    }

    pub fn quorum_sig(&self) -> Option<&AuthorityStrongQuorumSignInfo> {
        match self {
            ExecutionEffects::CertifiedTransactionEffects(certified_effects) => {
                Some(certified_effects.auth_sig())
            }
            ExecutionEffects::SuiTransactionEffects(_) => None,
        }
    }

    pub fn gas_object(&self) -> (ObjectRef, Owner) {
        match self {
            ExecutionEffects::CertifiedTransactionEffects(certified_effects) => {
                certified_effects.data().gas_object
            }
            ExecutionEffects::SuiTransactionEffects(sui_tx_effects) => {
                let refe = &sui_tx_effects.gas_object;
                (refe.reference.to_object_ref(), refe.owner)
            }
        }
    }
}

#[async_trait]
pub trait ValidatorProxy {
    async fn get_object(&self, object_id: ObjectID) -> Result<Object, anyhow::Error>;

    async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> anyhow::Result<(SuiCertifiedTransaction, ExecutionEffects)>;

    fn clone_committee(&self) -> Committee;

    fn get_current_epoch(&self) -> EpochId;

    fn clone_new(&self) -> Box<dyn ValidatorProxy + Send + Sync>;
}

pub struct LocalValidatorAggregatorProxy {
    _qd_handler: QuorumDriverHandler<NetworkAuthorityClient>,
    qd: Arc<QuorumDriver<NetworkAuthorityClient>>,
}

impl LocalValidatorAggregatorProxy {
    pub async fn from_genesis(
        genesis: &Genesis,
        registry: &Registry,
        fullnode_rpc_url: &str,
    ) -> Self {
        let (aggregator, _) = AuthorityAggregatorBuilder::from_genesis(genesis)
            .with_registry(registry)
            .build()
            .unwrap();

        let committee_store = aggregator.clone_committee_store();

        let reconfig_observer = FullNodeReconfigObserver::new(
            fullnode_rpc_url,
            committee_store,
            aggregator.safe_client_metrics_base.clone(),
            aggregator.metrics.clone(),
        )
        .await;
        let quorum_driver_metrics = Arc::new(QuorumDriverMetrics::new(registry));
        let qd_handler =
            QuorumDriverHandlerBuilder::new(Arc::new(aggregator), quorum_driver_metrics)
                .with_reconfig_observer(Arc::new(reconfig_observer))
                .start();

        let qd = qd_handler.clone_quorum_driver();
        Self {
            _qd_handler: qd_handler,
            qd,
        }
    }

    pub async fn from_network_config(
        configs: &NetworkConfig,
        registry: &Registry,
        fullnode_rpc_url: &str,
    ) -> Self {
        let (aggregator, _) = AuthorityAggregatorBuilder::from_network_config(configs)
            .with_registry(registry)
            .build()
            .unwrap();
        let committee_store = aggregator.clone_committee_store();

        let reconfig_observer = FullNodeReconfigObserver::new(
            fullnode_rpc_url,
            committee_store,
            aggregator.safe_client_metrics_base.clone(),
            aggregator.metrics.clone(),
        )
        .await;
        let quorum_driver_metrics = Arc::new(QuorumDriverMetrics::new(registry));
        let qd_handler =
            QuorumDriverHandlerBuilder::new(Arc::new(aggregator), quorum_driver_metrics)
                .with_reconfig_observer(Arc::new(reconfig_observer))
                .start();

        let qd = qd_handler.clone_quorum_driver();
        Self {
            _qd_handler: qd_handler,
            qd,
        }
    }
}

#[async_trait]
impl ValidatorProxy for LocalValidatorAggregatorProxy {
    async fn get_object(&self, object_id: ObjectID) -> Result<Object, anyhow::Error> {
        let auth_agg = self.qd.authority_aggregator().load();
        match auth_agg.get_object_info_execute(object_id).await? {
            ObjectRead::Exists(_, object, _) => Ok(object),
            other => bail!("object {object_id} does not exist: {:?}", other),
        }
    }

    async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> anyhow::Result<(SuiCertifiedTransaction, ExecutionEffects)> {
        let ticket = self.qd.submit_transaction(tx.verify()?).await?;
        let QuorumDriverResponse {
            tx_cert,
            effects_cert,
        } = ticket.await?;
        Ok((
            tx_cert.try_into().unwrap(),
            ExecutionEffects::CertifiedTransactionEffects(effects_cert.into()),
        ))
    }

    fn clone_committee(&self) -> Committee {
        self.qd.clone_committee()
    }

    fn get_current_epoch(&self) -> EpochId {
        self.qd.current_epoch()
    }

    fn clone_new(&self) -> Box<dyn ValidatorProxy + Send + Sync> {
        let qdh = self._qd_handler.clone_new();
        let qd = qdh.clone_quorum_driver();
        Box::new(Self {
            _qd_handler: qdh,
            qd,
        })
    }
}

pub struct FullNodeProxy {
    sui_client: SuiClient,
    committee: Committee,
}

impl FullNodeProxy {
    pub async fn from_url(http_url: &str) -> Result<Self, anyhow::Error> {
        let sui_client = SuiClient::new(http_url, None, None).await?;

        let resp = sui_client.read_api().get_committee_info(None).await?;
        let epoch = resp.epoch;
        let committee = if let Some(committee_vec) = resp.committee_info {
            let committee_map =
                BTreeMap::from_iter(committee_vec.into_iter().map(|(name, stake)| (name, stake)));
            Committee::new(epoch, committee_map)?
        } else {
            bail!(
                "Get empty committee info from fullnode for epoch {:?}",
                epoch
            )
        };

        Ok(Self {
            sui_client,
            committee,
        })
    }
}

#[async_trait]
impl ValidatorProxy for FullNodeProxy {
    async fn get_object(&self, object_id: ObjectID) -> Result<Object, anyhow::Error> {
        match self.sui_client.read_api().get_object(object_id).await? {
            SuiObjectRead::Exists(sui_obj) => sui_obj.try_into(),
            other => bail!("object {object_id} does not exist: {:?}", other),
        }
    }

    async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> anyhow::Result<(SuiCertifiedTransaction, ExecutionEffects)> {
        let result = self
            .sui_client
            .quorum_driver()
            // We need to use WaitForLocalExecution to make sure objects are updated on FN
            .execute_transaction(
                tx.verify()?,
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;
        let tx_cert = result.tx_cert.unwrap();
        let effects = ExecutionEffects::SuiTransactionEffects(result.effects.unwrap());
        Ok((tx_cert, effects))
    }

    fn clone_committee(&self) -> Committee {
        self.committee.clone()
    }

    fn get_current_epoch(&self) -> EpochId {
        self.committee.epoch
    }

    fn clone_new(&self) -> Box<dyn ValidatorProxy + Send + Sync> {
        Box::new(Self {
            sui_client: self.sui_client.clone(),
            committee: self.clone_committee(),
        })
    }
}
