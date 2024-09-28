use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use sui_json_rpc_types::{SuiObjectDataOptions, SuiObjectResponse, SuiRawData, SuiRawMovePackage};
use sui_sdk::{types::base_types::ObjectID, SuiClient};
use tokio::sync::RwLock;

pub struct PackageCache {
    rpc_client: SuiClient,
    cache: Arc<RwLock<BTreeMap<ObjectID, SuiRawMovePackage>>>,
}

impl PackageCache {
    pub fn new(rpc_client: SuiClient) -> Self {
        Self {
            rpc_client,
            cache: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    fn get_package_from_result(&self, obj_read: SuiObjectResponse) -> Result<SuiRawMovePackage> {
        let obj = obj_read
            .into_object()
            .map_err(|e| anyhow!("package object does not exist or was deleted: {}", e))?;
        let object_id = obj.object_id;
        let obj = obj.bcs.ok_or_else(|| anyhow!("bcs field not found"))?;
        match obj {
            SuiRawData::Package(pkg) => Ok(pkg),
            SuiRawData::MoveObject(_) => Err(anyhow!(
                "dependency ID contains a Sui object, not a Move package: {}",
                object_id
            )),
        }
    }

    pub async fn get_multi(&self, ids: Vec<ObjectID>) -> Result<Vec<Result<SuiRawMovePackage>>> {
        let mut res_map = BTreeMap::new();
        let mut to_fetch = Vec::new();

        {
            let cache = self.cache.read().await;
            for id in ids.iter() {
                if *id == ObjectID::ZERO {
                    res_map.insert(*id, Err(anyhow!("zero address")));
                } else if let Some(pkg) = cache.get(id) {
                    res_map.insert(*id, Ok(pkg.clone()));
                } else {
                    to_fetch.push(*id);
                }
            }
        }

        if !to_fetch.is_empty() {
            let fetch_res = self
                .rpc_client
                .read_api()
                .multi_get_object_with_options(
                    to_fetch.clone(),
                    SuiObjectDataOptions::new().with_bcs(),
                )
                .await?
                .into_iter()
                .map(|obj_read| self.get_package_from_result(obj_read))
                .collect::<Vec<Result<_>>>();

            let mut cache = self.cache.write().await;
            for (id, res) in to_fetch.into_iter().zip(fetch_res.into_iter()) {
                if let Ok(pkg) = &res {
                    cache.insert(id, pkg.clone());
                }
                res_map.insert(id, res);
            }
        };

        let ret = ids
            .into_iter()
            .map(|id| match res_map.get(&id) {
                Some(Ok(pkg)) => Ok(pkg.clone()),
                Some(Err(e)) => Err(anyhow!("error fetching package: {}", e)),
                None => Err(anyhow!("package not found for id: {}", id)),
            })
            .collect();

        Ok(ret)
    }

    pub async fn get(&mut self, id: ObjectID) -> Result<SuiRawMovePackage> {
        self.get_multi(vec![id]).await?.pop().unwrap()
    }
}

impl Clone for PackageCache {
    fn clone(&self) -> Self {
        Self {
            rpc_client: self.rpc_client.clone(),
            cache: self.cache.clone(),
        }
    }
}
