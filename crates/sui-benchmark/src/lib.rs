// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::bail;
use async_trait::async_trait;
use sui_core::{
    authority_aggregator::AuthorityAggregator,
    authority_client::NetworkAuthorityClient,
    quorum_driver::{QuorumDriver, QuorumDriverHandler, QuorumDriverMetrics},
};
use sui_network::default_mysten_network_config;
use sui_types::messages::QuorumDriverRequest;
use sui_types::{
    base_types::ObjectID,
    committee::{Committee, EpochId},
    error::SuiError,
    messages::{
        CertifiedTransaction, CertifiedTransactionEffects, QuorumDriverRequestType,
        QuorumDriverResponse, Transaction,
    },
    object::{Object, ObjectRead},
};
use tracing::{error, info};

pub mod drivers;
pub mod util;
pub mod workloads;

#[async_trait]
pub trait ValidatorProxy {
    async fn get_object(&self, object_id: ObjectID) -> Result<Object, anyhow::Error>;

    async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> Result<(CertifiedTransaction, CertifiedTransactionEffects), SuiError>;

    async fn reconfig(&self);

    async fn get_committee(&self) -> Committee;
}

pub struct LocalValidatorAggregator {
    // agg: Arc<AuthorityAggregator<NetworkAuthorityClient>>,
    _qd_handler: Arc<QuorumDriverHandler<NetworkAuthorityClient>>,
    qd: Arc<QuorumDriver<NetworkAuthorityClient>>,
}

impl LocalValidatorAggregator {
    pub fn from_auth_agg(agg: Arc<AuthorityAggregator<NetworkAuthorityClient>>) -> Self {
        let qd_handler = QuorumDriverHandler::new(agg, QuorumDriverMetrics::new_for_tests());
        let qd = qd_handler.clone_quorum_driver();
        Self {
            _qd_handler: Arc::new(qd_handler),
            qd,
        }
    }
}

#[async_trait]
impl ValidatorProxy for LocalValidatorAggregator {
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
    ) -> Result<(CertifiedTransaction, CertifiedTransactionEffects), SuiError> {
        match self
            .qd
            .execute_transaction(QuorumDriverRequest {
                transaction: tx,
                request_type: QuorumDriverRequestType::WaitForEffectsCert,
            })
            .await?
        {
            QuorumDriverResponse::EffectsCert(result) => {
                let (tx_cert, effects_cert) = *result;
                Ok((tx_cert, effects_cert))
            }
            other => panic!("This should not happen"),
            // Err(err) => Err(err),
        }
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
                        if let Err(err) = self.qd.update_validators(Arc::new(auth_agg)).await {
                            error!("Reconfiguration - Error when updating authority aggregator in quorum driver: {}", err);
                        } else {
                            info!("Reconfiguration - Reconfiguration to epoch {new_epoch} is done");
                        }
                    }
                }
            }
        }
    }

    async fn get_committee(&self) -> Committee {
        self.qd.clone_committee()
    }
}
