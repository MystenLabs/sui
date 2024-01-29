use std::{collections::BTreeMap, sync::Arc};

use anyhow::{anyhow, Result};
use move_core_types::account_address::AccountAddress;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiObjectResponse, SuiRawData, SuiRawMovePackage};
use sui_sdk::{apis::ReadApi, types::base_types::ObjectID};
use tokio::sync::RwLock;

pub struct PackageCache<'a> {
    rpc_client: &'a ReadApi,
    cache: Arc<RwLock<BTreeMap<AccountAddress, SuiRawMovePackage>>>,
}

impl<'a> PackageCache<'a> {
    pub fn new(rpc_client: &'a ReadApi) -> Self {
        Self {
            rpc_client,
            cache: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    fn get_package_from_result(&self, obj_read: SuiObjectResponse) -> Result<SuiRawMovePackage> {
        let obj = obj_read
            .into_object()
            .map_err(|e| anyhow!("package object does not exist or was deleted: {}", e))?;
        let addr = AccountAddress::from(obj.object_id);
        let obj = obj.bcs.ok_or_else(|| anyhow!("bcs field not found"))?;
        match obj {
            SuiRawData::Package(pkg) => Ok(pkg),
            SuiRawData::MoveObject(_) => Err(anyhow!(
                "dependency ID contains a Sui object, not a Move package: {}",
                addr
            )),
        }
    }

    pub async fn get_multi(
        &mut self,
        addrs: Vec<AccountAddress>,
    ) -> Result<Vec<Result<SuiRawMovePackage>>> {
        let cache = self.cache.read().await;
        let have = addrs
            .iter()
            .map(|addr| match cache.get(addr) {
                Some(package) => (ObjectID::from(*addr), Some(package.clone())),
                None => (ObjectID::from(*addr), None),
            })
            .collect::<Vec<_>>();
        drop(cache);

        let to_fetch = have
            .iter()
            .filter_map(|(addr, pkg)| match pkg {
                Some(_) => None,
                None => Some(*addr),
            })
            .collect::<Vec<_>>();

        let mut fetch_res = self
            .rpc_client
            .multi_get_object_with_options(to_fetch, SuiObjectDataOptions::new().with_bcs())
            .await?
            .into_iter()
            .map(|obj_read| self.get_package_from_result(obj_read))
            .collect::<Vec<Result<_>>>();

        let mut res = vec![];
        let mut cache = self.cache.write().await;
        for (addr, pkg) in have.into_iter() {
            match pkg {
                Some(pkg) => res.push(Ok(pkg)),
                None => {
                    let pkg_res = fetch_res.remove(0);
                    if let Ok(pkg) = &pkg_res {
                        cache.insert(*addr, pkg.clone());
                    }
                    res.push(pkg_res);
                }
            }
        }

        Ok(res)
    }

    pub async fn get(&mut self, addr: AccountAddress) -> Result<SuiRawMovePackage> {
        self.get_multi(vec![addr]).await?.pop().unwrap()
    }
}

impl Clone for PackageCache<'_> {
    fn clone(&self) -> Self {
        Self {
            rpc_client: self.rpc_client,
            cache: self.cache.clone(),
        }
    }
}
