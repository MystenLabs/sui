use std::collections::{BTreeMap, HashSet};

use anyhow::{anyhow, Result};
use sui_json_rpc_types::{SuiObjectDataOptions, SuiRawData, SuiRawMovePackage};
use sui_sdk::{types::base_types::ObjectID, SuiClient};

pub struct PackageCache {
    client: SuiClient,
    cache: BTreeMap<ObjectID, SuiRawMovePackage>,
}

impl PackageCache {
    pub fn new(client: SuiClient) -> Self {
        Self {
            client,
            cache: BTreeMap::new(),
        }
    }

    async fn fetch_missing(&mut self, ids: &[ObjectID]) -> Result<()> {
        let to_fetch = ids
            .into_iter()
            .filter(|id| !self.cache.contains_key(id))
            .cloned()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        if to_fetch.is_empty() {
            return Ok(());
        }

        let fetch_res = self
            .client
            .read_api()
            .multi_get_object_with_options(to_fetch, SuiObjectDataOptions::new().with_bcs())
            .await?;
        for obj_read in fetch_res {
            let obj = obj_read
                .into_object()
                .map_err(|e| anyhow!("package object does not exist or was deleted: {}", e))?;
            let id = obj.object_id;
            let obj = obj.bcs.ok_or_else(|| anyhow!("bcs field not found"))?;
            match obj {
                SuiRawData::Package(pkg) => {
                    self.cache.insert(id, pkg);
                }
                SuiRawData::MoveObject(_) => {
                    return Err(anyhow!(
                        "dependency ID contains a Sui object, not a Move package: {}",
                        id
                    ));
                }
            }
        }

        Ok(())
    }

    pub async fn get_multi(&mut self, ids: Vec<ObjectID>) -> Result<Vec<SuiRawMovePackage>> {
        self.fetch_missing(&ids).await?;

        let pkgs = ids
            .into_iter()
            .map(|id| self.cache.get(&id).cloned().unwrap())
            .collect();
        Ok(pkgs)
    }

    pub async fn get(&mut self, id: ObjectID) -> Result<SuiRawMovePackage> {
        self.fetch_missing(&[id]).await?;

        let pkg = self.cache.get(&id).cloned().unwrap();
        Ok(pkg)
    }
}
