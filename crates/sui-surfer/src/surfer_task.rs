// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use rand::{rngs::StdRng, Rng, SeedableRng};
use sui_core::authority::authority_store_tables::LiveObject;
use sui_types::{
    base_types::{ObjectRef, SuiAddress},
    object::Owner,
};
use test_utils::network::TestCluster;
use tokio::sync::{watch, RwLock};

use crate::{
    surf_strategy::SurfStrategy,
    surfer_state::{ImmObjects, OwnedObjects, SharedObjects, SurfStatistics, SurferState},
};

pub struct SurferTask {
    pub state: SurferState,
    pub surf_strategy: Box<dyn SurfStrategy>,
    pub exit_rcv: watch::Receiver<()>,
}

impl SurferTask {
    pub async fn create_surfer_tasks<S: Default + SurfStrategy>(
        cluster: Arc<TestCluster>,
        seed: u64,
        exit_rcv: watch::Receiver<()>,
    ) -> Vec<SurferTask> {
        let mut rng = StdRng::seed_from_u64(seed);
        let immutable_objects: ImmObjects = Arc::new(RwLock::new(HashMap::new()));
        let shared_objects: SharedObjects = Arc::new(RwLock::new(HashMap::new()));

        let mut accounts: HashMap<SuiAddress, (Option<ObjectRef>, OwnedObjects)> = cluster
            .accounts
            .iter()
            .map(|address| (*address, (None, HashMap::new())))
            .collect();
        let validator = cluster
            .swarm
            .validators()
            .next()
            .unwrap()
            .get_node_handle()
            .unwrap();
        let all_live_objects: Vec<_> =
            validator.with(|node| node.state().db().iter_live_object_set().collect());
        for obj in all_live_objects {
            match obj {
                LiveObject::Normal(obj) => {
                    if let Some(struct_tag) = obj.struct_tag() {
                        let obj_ref = obj.compute_object_reference();
                        match obj.owner {
                            Owner::Immutable => {
                                immutable_objects
                                    .write()
                                    .await
                                    .entry(struct_tag)
                                    .or_default()
                                    .push(obj_ref);
                            }
                            Owner::Shared {
                                initial_shared_version,
                            } => {
                                shared_objects
                                    .write()
                                    .await
                                    .entry(struct_tag)
                                    .or_default()
                                    .push((obj_ref.0, initial_shared_version));
                            }
                            Owner::AddressOwner(address) => {
                                if let Some((gas_object, owned_objects)) =
                                    accounts.get_mut(&address)
                                {
                                    if obj.is_gas_coin() && gas_object.is_none() {
                                        gas_object.replace(obj_ref);
                                    } else {
                                        owned_objects
                                            .entry(struct_tag)
                                            .or_default()
                                            .insert(obj_ref);
                                    }
                                }
                            }
                            Owner::ObjectOwner(_) => (),
                        }
                    }
                }
                LiveObject::Wrapped(_) => (),
            }
        }
        let entry_functions = Arc::new(RwLock::new(vec![]));
        accounts
            .into_iter()
            .map(|(address, (gas_object, owned_objects))| {
                let seed = rng.gen::<u64>();
                let state_rng = StdRng::seed_from_u64(seed);
                let state = SurferState::new(
                    cluster.clone(),
                    state_rng,
                    address,
                    gas_object.unwrap(),
                    owned_objects,
                    immutable_objects.clone(),
                    shared_objects.clone(),
                    entry_functions.clone(),
                );
                SurferTask {
                    state,
                    surf_strategy: Box::<S>::default(),
                    exit_rcv: exit_rcv.clone(),
                }
            })
            .collect()
    }

    pub async fn surf(mut self) -> SurfStatistics {
        loop {
            let entry_functions = self.state.entry_functions.read().await.clone();
            self.surf_strategy
                .surf_for_a_while(&mut self.state, entry_functions, &self.exit_rcv)
                .await;
            if self.exit_rcv.has_changed().unwrap() {
                return self.state.stats;
            }
        }
    }
}
