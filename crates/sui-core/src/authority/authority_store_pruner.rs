// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::Rng;
use std::{sync::Arc, time::Duration};
use sui_types::base_types::{ObjectID, SequenceNumber, VersionNumber};
use tokio::sync::oneshot::{self, Sender};
use tracing::log::{error, info};
use typed_store::Map;

use crate::authority::authority_store::ObjectKey;

use super::authority_store_tables::AuthorityPerpetualTables;

const OBJECTS_PRUNING_PERIOD_MINS: u64 = 60;
const MAX_PENDING_OPS_IN_ONE_WRITE_BATCH: u64 = 10000;

pub struct AuthorityStorePruner {
    _objects_pruner_cancel_handle: oneshot::Sender<()>,
}

impl AuthorityStorePruner {
    fn setup_objects_pruning(perpetual_db: Arc<AuthorityPerpetualTables>) -> Sender<()> {
        let mut interval =
            tokio::time::interval(Duration::from_secs(OBJECTS_PRUNING_PERIOD_MINS * 60));
        let (sender, mut recv) = tokio::sync::oneshot::channel();
        let mut rng = rand::thread_rng();
        let rand = rng.gen::<u32>();
        tokio::task::spawn(async move {
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if rand % 2 == 0 {
                            info!("Skipping pruning of objects table");
                            continue;
                        }
                        info!("Starting pruning of objects table");
                        let perpetual_db = perpetual_db.clone();
                        let iter = perpetual_db.objects.iter().skip_to_last().reverse();
                        let mut total_keys_scanned = 0;
                        let mut total_objects_scanned = 0;
                        let mut num_versions_for_key = 0;
                        let mut object_id: Option<ObjectID> = None;
                        let mut seq_num: Option<VersionNumber> = None;
                        let mut num_pending_wb_ops = 0;
                        let mut wb = perpetual_db.objects.batch();
                        for (key, _value) in iter {
                            total_keys_scanned += 1;
                            if let Some(obj_id) = object_id {
                                if obj_id != key.0 {
                                    total_objects_scanned += 1;
                                    if let Some(seq) = seq_num {
                                        // delete keys in range [(obj_id, VersionNumber::ZERO), (obj_id, seq))
                                        let start = ObjectKey(obj_id, SequenceNumber::from_u64(0));
                                        let end = ObjectKey(obj_id, seq);
                                        if let Ok(new_wb) = wb.delete_range(&perpetual_db.objects, &start, &end) {
                                            wb = new_wb;
                                            num_pending_wb_ops += 1;
                                        } else {
                                            error!("Failed to invoke delete_range on write batch while compacting objects");
                                            wb = perpetual_db.objects.batch();
                                            num_pending_wb_ops = 0;
                                            break;
                                        }
                                        if num_pending_wb_ops >= MAX_PENDING_OPS_IN_ONE_WRITE_BATCH {
                                            if wb.write().is_err() {
                                                error!("Failed to commit write batch while compacting objects");
                                                wb = perpetual_db.objects.batch();
                                                num_pending_wb_ops = 0;
                                                break;
                                            } else {
                                                info!("Committed write batch while compacting objects, keys scanned = {:?}, objects scanned = {:?}", total_keys_scanned, total_objects_scanned);
                                            }
                                            wb = perpetual_db.objects.batch();
                                            num_pending_wb_ops = 0;
                                        }
                                    }
                                    num_versions_for_key = 0;
                                    object_id = Some(key.0);
                                    seq_num = None;
                                }
                                num_versions_for_key += 1;
                                // We'll keep maximum 2 latest version of any object
                                if num_versions_for_key == 2 {
                                    seq_num = Some(key.1);
                                }
                            } else {
                                num_versions_for_key = 1;
                                total_objects_scanned = 1;
                                object_id = Some(key.0);
                            }
                        }
                        if num_pending_wb_ops > 0 {
                            if wb.write().is_err() {
                                error!("Failed to commit write batch while compacting objects");
                            } else {
                                info!("Committed write batch while compacting objects, keys scanned = {:?}, objects scanned = {:?}", total_keys_scanned, total_objects_scanned);
                            }
                        }
                        info!("Finished compacting objects, keys scanned = {:?}, objects scanned = {:?}", total_keys_scanned, total_objects_scanned);
                    }
                    _ = &mut recv => break,
                }
            }
        });
        sender
    }
    pub fn new(perpetual_db: Arc<AuthorityPerpetualTables>) -> Self {
        AuthorityStorePruner {
            _objects_pruner_cancel_handle: Self::setup_objects_pruning(perpetual_db),
        }
    }
}
