// // Copyright (c) 2022, Mysten Labs, Inc.
// // SPDX-License-Identifier: Apache-2.0

// use anyhow::anyhow;
// use std::{
//     collections::{BTreeMap, BTreeSet},
//     path::PathBuf,
//     sync::Arc,
// };
// use tokio::sync::mpsc::channel;

// use move_binary_format::CompiledModule;
// use move_package::BuildConfig;
// use sui_adapter::{adapter::generate_package_id, genesis};
// use sui_config::{sui_config_dir, GenesisConfig, NetworkConfig};
// use sui_core::{
//     authority::{AuthorityState, AuthorityStore, ReplicaStore},
//     authority_server::AuthorityServer,
//     full_node::FullNodeState,
// };
// use sui_types::{
//     base_types::{ObjectID, SequenceNumber, SuiAddress, TxContext},
//     committee::Committee,
//     error::SuiResult,
//     object::Object,
// };
// use tracing::info;

// use crate::keystore::{Keystore, SuiKeystore};

// pub struct GenesisState {
//     pub committee: Committee,
//     pub network_config: NetworkConfig,
//     pub addresses: Vec<SuiAddress>,
//     pub keystore: SuiKeystore,
//     pub preload_modules: Vec<Vec<CompiledModule>>,
//     pub preload_objects: Vec<Object>,
//     pub tx_ctx: TxContext,
// }

// impl GenesisState {
//     pub async fn new_from_config(genesis_conf: GenesisConfig) -> Result<Self, anyhow::Error> {
//         let config_dir = sui_config_dir().unwrap();
//         let mut network_config = NetworkConfig::generate(&config_dir, genesis_conf.committee_size);

//         let mut addresses = Vec::new();
//         let mut preload_modules: Vec<Vec<CompiledModule>> = Vec::new();
//         let mut preload_objects = Vec::new();
//         let mut all_preload_objects_set = BTreeSet::new();

//         info!("Creating accounts and gas objects...",);

//         let mut keystore = SuiKeystore::default();
//         for account in genesis_conf.accounts {
//             let address = if let Some(address) = account.address {
//                 address
//             } else {
//                 keystore.add_random_key()?
//             };

//             addresses.push(address);
//             let mut preload_objects_map = BTreeMap::new();

//             // Populate gas itemized objects
//             account.gas_objects.iter().for_each(|q| {
//                 if !all_preload_objects_set.contains(&q.object_id) {
//                     preload_objects_map.insert(q.object_id, q.gas_value);
//                 }
//             });

//             // Populate ranged gas objects
//             if let Some(ranges) = account.gas_object_ranges {
//                 for rg in ranges {
//                     let ids = ObjectID::in_range(rg.offset, rg.count)?;

//                     for obj_id in ids {
//                         if !preload_objects_map.contains_key(&obj_id)
//                             && !all_preload_objects_set.contains(&obj_id)
//                         {
//                             preload_objects_map.insert(obj_id, rg.gas_value);
//                             all_preload_objects_set.insert(obj_id);
//                         }
//                     }
//                 }
//             }

//             for (object_id, value) in preload_objects_map {
//                 let new_object = Object::with_id_owner_gas_coin_object_for_testing(
//                     object_id,
//                     SequenceNumber::new(),
//                     address,
//                     value,
//                 );
//                 preload_objects.push(new_object);
//             }
//         }

//         info!(
//             "Loading Move framework lib from {:?}",
//             genesis_conf.move_framework_lib_path
//         );
//         let move_lib =
//             sui_framework::get_move_stdlib_modules(&genesis_conf.move_framework_lib_path)?;
//         preload_modules.push(move_lib);

//         // Load Sui and Move framework lib
//         info!(
//             "Loading Sui framework lib from {:?}",
//             genesis_conf.sui_framework_lib_path
//         );
//         let sui_lib =
//             sui_framework::get_sui_framework_modules(&genesis_conf.sui_framework_lib_path)?;
//         preload_modules.push(sui_lib);

//         // TODO: allow custom address to be used after the Gateway refactoring
//         // Default to use the last address in the wallet config for initializing modules.
//         // If there's no address in wallet config, then use 0x0
//         let null_address = SuiAddress::default();
//         let module_init_address = addresses.last().unwrap_or(&null_address);
//         let mut genesis_ctx = genesis::get_genesis_context_with_custom_address(module_init_address);
//         // Build custom move packages
//         if !genesis_conf.move_packages.is_empty() {
//             info!(
//                 "Loading {} Move packages from {:?}",
//                 &genesis_conf.move_packages.len(),
//                 &genesis_conf.move_packages
//             );

//             for path in genesis_conf.move_packages {
//                 let mut modules =
//                     sui_framework::build_move_package(&path, BuildConfig::default(), false)?;

//                 let package_id = generate_package_id(&mut modules, &mut genesis_ctx)?;

//                 info!("Loaded package [{}] from {:?}.", package_id, path);
//                 // Writing package id to network config for user to retrieve later.
//                 network_config.add_move_package(path, package_id);
//                 preload_modules.push(modules)
//             }
//         }

//         let committee = match network_config.validator_configs().iter().next() {
//             Some(q) => q.committee_config().committee(),
//             None => return Err(anyhow!("Validator configs must exist")),
//         };
//         Ok(Self {
//             committee,
//             network_config,
//             addresses,
//             keystore,
//             preload_modules,
//             preload_objects,
//             tx_ctx: genesis_ctx,
//         })
//     }

//     pub async fn make_full_node_state_with_genesis_ctx(
//         &mut self,
//         path: PathBuf,
//     ) -> SuiResult<FullNodeState> {
//         let store = Arc::new(ReplicaStore::open(path, None));
//         let state = FullNodeState::new(
//             self.committee.clone(),
//             store,
//             self.preload_modules.clone(),
//             &mut self.tx_ctx,
//         )
//         .await?;

//         // Okay to do this since we're at genesis
//         state
//             .insert_genesis_objects_bulk_unsafe(&self.preload_objects.iter().collect::<Vec<_>>())
//             .await;
//         Ok(state)
//     }

//     pub async fn populate_authority_with_genesis_ctx(
//         &self,
//         validator_config_index: usize,
//     ) -> SuiResult<AuthorityServer> {
//         let validator_config = self
//             .network_config
//             .validator_configs()
//             .get(validator_config_index)
//             .expect("No validator config found at index {validator_config_index}");

//         let store = Arc::new(AuthorityStore::open(validator_config.db_path(), None));
//         let name = *validator_config.key_pair().public_key_bytes();

//         let state = AuthorityState::new(
//             validator_config.committee_config().committee(),
//             name,
//             Arc::pin(validator_config.key_pair().copy()),
//             store,
//             self.preload_modules.clone(),
//             &mut self.tx_ctx.clone(),
//         )
//         .await;

//         // Okay to do this since we're at genesis
//         state
//             .insert_genesis_objects_bulk_unsafe(&self.preload_objects.iter().collect::<Vec<_>>())
//             .await;

//         let (tx_sui_to_consensus, _rx_sui_to_consensus) = channel(1);
//         Ok(AuthorityServer::new(
//             validator_config.network_address().clone(),
//             Arc::new(state),
//             validator_config.consensus_config().address().clone(),
//             /* tx_consensus_listener */ tx_sui_to_consensus,
//         ))
//     }
// }
