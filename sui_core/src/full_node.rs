// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::{collections::HashMap, sync::Arc};

use sui_types::base_types::AuthorityName;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::error::SuiResult;
use sui_types::object::ObjectRead;
use tokio::sync::Mutex as TokioMutex;
use tokio::sync::Mutex;
use tracing::debug;

use crate::authority_client::AuthorityClient;
use crate::{authority_active::AuthorityHealth, authority_aggregator::AuthorityAggregator};
use futures::channel::mpsc::channel as MpscChannel;
mod follower;
pub use crate::full_node::follower::follow_multiple;

mod full_node_state;
pub use crate::full_node::full_node_state::FullNodeState;

use self::follower::Downloader;

const DOWNLOADER_CHANNEL_SIZE: usize = 1024;

pub struct FullNode {
    // Local state
    pub state: Arc<FullNodeState>,

    // The network interfaces to authorities
    pub aggregator: Arc<AuthorityAggregator>,

    // Network health
    pub health: Arc<TokioMutex<HashMap<AuthorityName, AuthorityHealth>>>,
}

impl FullNode {
    // TODO: Provide way to run genesis here

    pub fn new(
        state: Arc<FullNodeState>,
        authority_clients: BTreeMap<AuthorityName, AuthorityClient>,
    ) -> SuiResult<Self> {
        let committee = state.committee.clone();

        Ok(Self {
            health: Arc::new(Mutex::new(
                committee
                    .clone()
                    .voting_rights
                    .iter()
                    .map(|(name, _)| (*name, AuthorityHealth::default()))
                    .collect(),
            )),
            state,
            aggregator: Arc::new(AuthorityAggregator::new(committee, authority_clients)),
        })
    }

    async fn download_object_from_authorities(&self, object_id: ObjectID) -> SuiResult<ObjectRead> {
        let result = self.aggregator.get_object_info_execute(object_id).await?;
        if let ObjectRead::Exists(obj_ref, object, _) = &result {
            let local_object = self.state.store.get_object(&object_id)?;
            if local_object.is_none()
                || &local_object.unwrap().compute_object_reference() != obj_ref
            {
                self.state
                    .store
                    .insert_object_direct(*obj_ref, object)
                    .await?;
            }
        }
        debug!(?result, "Downloaded object from authorities");

        Ok(result)
    }

    pub async fn get_object_info(&self, object_id: ObjectID) -> Result<ObjectRead, anyhow::Error> {
        let result = self.download_object_from_authorities(object_id).await?;
        Ok(result)
    }

    pub async fn get_owned_objects(
        &self,
        account_addr: SuiAddress,
    ) -> Result<Vec<ObjectRef>, anyhow::Error> {
        Ok(self.state.store.get_account_objects(account_addr)?)
    }
}

impl FullNode {
    pub async fn spawn_tasks(&self) {
        let (send_chann, recv_chann) = MpscChannel(DOWNLOADER_CHANNEL_SIZE);

        let downloader = Downloader {
            aggregator: Arc::new(self.aggregator.as_ref().clone()),
            state: self.state.clone(),
        };

        // Spawn a downloader
        downloader.start_downloader(recv_chann).await;

        // Spawn follower tasks
        follow_multiple(self, self.state.committee.quorum_threshold(), send_chann).await;
    }
}
