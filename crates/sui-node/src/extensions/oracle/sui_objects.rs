use std::sync::Arc;

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

use move_core_types::account_address::AccountAddress;
use sui_exex::context::ExExStore;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    collection_types::{VecMap, VecSet},
    id::{ID, UID},
    object::Data,
    storage::ObjectStore,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PuiPriceStorage {
    pub id: UID,
    pub publisher_name: String,
    pub price: Option<u128>,
    pub timestamp: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PuiPublisher {
    pub name: String,
    pub address: SuiAddress,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PuiRegistry {
    pub id: UID,
    pub owner: SuiAddress,
    pub publishers: VecSet<PuiPublisher>,
    pub publishers_storages: VecMap<SuiAddress, ID>,
}

pub fn deserialize_object<'a, T: Deserialize<'a>>(
    store: &Arc<dyn ExExStore>,
    address: AccountAddress,
) -> anyhow::Result<T> {
    let object = store
        .get_object(&ObjectID::from_address(address))
        .context("Fetching the object")?;

    match object.as_inner().data.clone() {
        Data::Move(o) => {
            let boxed_contents = Box::leak(o.contents().to_vec().into_boxed_slice());
            Ok(bcs::from_bytes(boxed_contents)?)
        }
        Data::Package(_) => bail!("Object should not be a Package"),
    }
}

pub fn deserialize_objects<'a, T: Deserialize<'a>>(
    store: &Arc<dyn ExExStore>,
    ids: &[ObjectID],
) -> anyhow::Result<Vec<T>> {
    let objects = store.multi_get_objects(&ids);

    objects
        .into_iter()
        .map(|obj| {
            let object = obj.context("Object not found")?;
            match object.as_inner().data.clone() {
                Data::Move(o) => {
                    let boxed_contents = Box::leak(o.contents().to_vec().into_boxed_slice());
                    bcs::from_bytes(boxed_contents).map_err(anyhow::Error::from)
                }
                Data::Package(_) => bail!("Object should not be a Package"),
            }
        })
        .collect()
}
