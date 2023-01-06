// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use anyhow::bail;
use async_trait::async_trait;
use std::{collections::BTreeMap, sync::Arc};
use sui_core::{
    authority_aggregator::AuthorityAggregator,
    authority_client::NetworkAuthorityClient,
    quorum_driver::reconfig_observer::DummyReconfigObserver,
    quorum_driver::{
        QuorumDriver, QuorumDriverHandler, QuorumDriverHandlerBuilder, QuorumDriverMetrics,
    },
};
use sui_json_rpc_types::{SuiCertifiedTransaction, SuiObjectRead, SuiTransactionEffects};
use sui_network::default_mysten_network_config;
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
use tracing::{error, info};

pub mod drivers;
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

    async fn reconfig(&self);

    fn clone_committee(&self) -> Committee;

    fn get_current_epoch(&self) -> EpochId;

    fn clone_new(&self) -> Box<dyn ValidatorProxy + Send + Sync>;
}

pub struct LocalValidatorAggregatorProxy {
    _qd_handler: QuorumDriverHandler<NetworkAuthorityClient>,
    qd: Arc<QuorumDriver<NetworkAuthorityClient>>,
}

impl LocalValidatorAggregatorProxy {
    pub fn from_auth_agg(agg: Arc<AuthorityAggregator<NetworkAuthorityClient>>) -> Self {
        let qd_handler =
            QuorumDriverHandlerBuilder::new(agg, Arc::new(QuorumDriverMetrics::new_for_tests()))
                // TODO: replace with a real reconfig observer
                .with_reconfig_observer(Arc::new(DummyReconfigObserver {}))
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

    async fn reconfig(&self) {
        let auth_agg = self.qd.authority_aggregator().load();
        match auth_agg
            .get_committee_with_net_addresses(self.qd.current_epoch())
            .await
        {
            Err(err) => {
                error!(
                    "Reconfiguration - Failed to get committee with network address: {}",
                    err
                )
            }
            Ok(committee_info) => {
                let network_config = default_mysten_network_config();
                let new_epoch = committee_info.committee.epoch;
                // Check if we already advanced.
                let cur_epoch = self.qd.current_epoch();
                if new_epoch <= cur_epoch {
                    return;
                }
                info!("Reconfiguration - Observed a new epoch {new_epoch}, attempting to reconfig from current epoch: {cur_epoch}");
                match auth_agg.recreate_with_net_addresses(committee_info, &network_config) {
                    Err(err) => error!(
                        "Reconfiguration - Error when cloning authority aggregator with committee: {}",
                        err
                    ),
                    Ok(auth_agg) => {
                        self.qd.update_validators(Arc::new(auth_agg)).await;
                        info!("Reconfiguration - Reconfiguration to epoch {new_epoch} is done");
                    }
                }
            }
        }
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

    async fn reconfig(&self) {
        // TODO poll FN until it has proceeds to next epoch
        // and update self.committee
        return;
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
