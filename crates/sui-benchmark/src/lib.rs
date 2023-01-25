// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::bail;
use async_trait::async_trait;
use embedded_reconfig_observer::EmbeddedReconfigObserver;
use fullnode_reconfig_observer::FullNodeReconfigObserver;
use prometheus::Registry;
use std::{collections::BTreeMap, sync::Arc};
use sui_config::genesis::Genesis;
use sui_config::NetworkConfig;
use sui_core::{
    authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder},
    authority_client::NetworkAuthorityClient,
    quorum_driver::{
        QuorumDriver, QuorumDriverHandler, QuorumDriverHandlerBuilder, QuorumDriverMetrics,
    },
};
use sui_json_rpc_types::{SuiCertifiedTransaction, SuiObjectRead, SuiTransactionEffects};
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::SuiAddress;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::{
    base_types::ObjectID,
    committee::{Committee, EpochId},
    messages::{CertifiedTransactionEffects, QuorumDriverResponse, Transaction},
    object::{Object, ObjectRead},
    SUI_SYSTEM_STATE_OBJECT_ID,
};
use sui_types::{
    base_types::ObjectRef, crypto::AuthorityStrongQuorumSignInfo,
    messages::ExecuteTransactionRequestType, object::Owner,
};
use tracing::{error, info};

pub mod benchmark_setup;
pub mod drivers;
pub mod embedded_reconfig_observer;
pub mod fullnode_reconfig_observer;
pub mod options;
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

    async fn get_validators(&self) -> Result<Vec<SuiAddress>, anyhow::Error>;
}

pub struct LocalValidatorAggregatorProxy {
    _qd_handler: QuorumDriverHandler<NetworkAuthorityClient>,
    qd: Arc<QuorumDriver<NetworkAuthorityClient>>,
}

impl LocalValidatorAggregatorProxy {
    pub async fn from_genesis(
        genesis: &Genesis,
        registry: &Registry,
        reconfig_fullnode_rpc_url: Option<&str>,
    ) -> Self {
        let (aggregator, _) = AuthorityAggregatorBuilder::from_genesis(genesis)
            .with_registry(registry)
            .build()
            .unwrap();

        Self::new_impl(aggregator, registry, reconfig_fullnode_rpc_url).await
    }

    pub async fn from_network_config(
        configs: &NetworkConfig,
        registry: &Registry,
        reconfig_fullnode_rpc_url: Option<&str>,
    ) -> Self {
        let (aggregator, _) = AuthorityAggregatorBuilder::from_network_config(configs)
            .with_registry(registry)
            .build()
            .unwrap();
        Self::new_impl(aggregator, registry, reconfig_fullnode_rpc_url).await
    }

    async fn new_impl(
        aggregator: AuthorityAggregator<NetworkAuthorityClient>,
        registry: &Registry,
        reconfig_fullnode_rpc_url: Option<&str>,
    ) -> Self {
        let quorum_driver_metrics = Arc::new(QuorumDriverMetrics::new(registry));
        let qd_handler_builder =
            QuorumDriverHandlerBuilder::new(Arc::new(aggregator.clone()), quorum_driver_metrics);

        let qd_handler = (if let Some(reconfig_fullnode_rpc_url) = reconfig_fullnode_rpc_url {
            info!(
                "Using FullNodeReconfigObserver: {:?}",
                reconfig_fullnode_rpc_url
            );
            let committee_store = aggregator.clone_committee_store();
            let reconfig_observer = Arc::new(
                FullNodeReconfigObserver::new(
                    reconfig_fullnode_rpc_url,
                    committee_store,
                    aggregator.safe_client_metrics_base.clone(),
                    aggregator.metrics.clone(),
                )
                .await,
            );
            qd_handler_builder.with_reconfig_observer(reconfig_observer)
        } else {
            info!("Using EmbeddedReconfigObserver");
            qd_handler_builder.with_reconfig_observer(Arc::new(EmbeddedReconfigObserver::new()))
        })
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
        let tx_digest = *tx.digest();
        let tx = tx.verify()?;
        let mut retry_cnt = 0;
        while retry_cnt < 3 {
            let ticket = self.qd.submit_transaction(tx.clone()).await?;
            // The ticket only times out when QuorumDriver exceeds the retry times
            match ticket.await {
                Ok(resp) => {
                    let QuorumDriverResponse {
                        tx_cert,
                        effects_cert,
                    } = resp;
                    return Ok((
                        tx_cert.try_into().unwrap(),
                        ExecutionEffects::CertifiedTransactionEffects(effects_cert.into()),
                    ));
                }
                Err(err) => {
                    error!(
                        ?tx_digest,
                        retry_cnt, "Transaction failed with err: {:?}", err
                    );
                    retry_cnt += 1;
                }
            }
        }
        bail!("Transaction {:?} failed for {retry_cnt} times", tx_digest);
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

    async fn get_validators(&self) -> Result<Vec<SuiAddress>, anyhow::Error> {
        let system_state = self.get_object(SUI_SYSTEM_STATE_OBJECT_ID).await?;
        let move_obj = system_state.data.try_as_move().unwrap();
        let result = bcs::from_bytes::<SuiSystemState>(move_obj.contents())?;
        Ok(result
            .validators
            .active_validators
            .into_iter()
            .map(|v| v.metadata.sui_address)
            .collect())
    }
}

pub struct FullNodeProxy {
    sui_client: SuiClient,
    committee: Committee,
}

impl FullNodeProxy {
    pub async fn from_url(http_url: &str) -> Result<Self, anyhow::Error> {
        // Each request times out after 60s (default value)
        let sui_client = SuiClientBuilder::default().build(http_url).await?;

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
        let tx_digest = *tx.digest();
        let tx = tx.verify()?;
        let mut retry_cnt = 0;
        while retry_cnt < 10 {
            // Fullnode could time out after WAIT_FOR_FINALITY_TIMEOUT (30s) in TransactionOrchestrator
            // SuiClient times out after 60s
            match self
                .sui_client
                .quorum_driver()
                .execute_transaction(
                    tx.clone(),
                    // We need to use WaitForLocalExecution to make sure objects are updated on FN
                    Some(ExecuteTransactionRequestType::WaitForLocalExecution),
                )
                .await
            {
                Ok(resp) => {
                    let tx_cert = resp.tx_cert.unwrap();
                    let effects = ExecutionEffects::SuiTransactionEffects(resp.effects.unwrap());
                    return Ok((tx_cert, effects));
                }
                Err(err) => {
                    error!(
                        ?tx_digest,
                        retry_cnt, "Transaction failed with err: {:?}", err
                    );
                    retry_cnt += 1;
                }
            }
        }
        bail!("Transaction {:?} failed for {retry_cnt} times", tx_digest);
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

    async fn get_validators(&self) -> Result<Vec<SuiAddress>, anyhow::Error> {
        let validators = self.sui_client.governance_api().get_validators().await?;
        Ok(validators.into_iter().map(|v| v.sui_address).collect())
    }
}
