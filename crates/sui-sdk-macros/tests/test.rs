use crate::bridge::bridge::BridgeInner;
use crate::sui::coin::Coin;
use crate::sui::sui::SUI;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use sui_sdk::SuiClientBuilder;
use sui_sdk_macros::move_contract;
use sui_sdk_types::ObjectId;

move_contract! {alias = "move_stdlib", package = "0x1"}
move_contract! {alias = "sui", package = "0x2"}

move_contract! {alias = "bridge", package = "0xb"}
move_contract! {alias = "mvr_metadata", package = "@mvr/metadata"}

move_contract! {alias = "suins", package = "0xd22b24490e0bae52676651b4f56660a5ff8022a2576e0089f79b3c88d44e08f0"}

use crate::suins::*;
move_contract! {alias = "mvr_core", package = "@mvr/core"}

#[tokio::test]
async fn test() {
    let client = SuiClientBuilder::default()
        .build("https://rpc.mainnet.sui.io:443")
        .await
        .unwrap();

    let bridge_bcs = client
        .read_api()
        .get_move_object_bcs(
            ObjectId::from_str(
                "0x00ba8458097a879607d609817a05599dc3e9e73ce942f97d4f1262605a8bf0fc",
            )
            .unwrap()
            .into(),
        )
        .await
        .unwrap();

    let bridge: Field<u64, BridgeInner> = bcs::from_bytes(&bridge_bcs).unwrap();

    println!("{}", Coin::<SUI>::type_(vec![SUI::type_().into()]));
}

#[derive(Clone, Serialize, Deserialize, Debug)]
struct Field<N, V> {
    pub id: ObjectId,
    pub name: N,
    pub value: V,
}
