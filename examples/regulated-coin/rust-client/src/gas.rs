// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, anyhow};
use tracing::debug;

use sui_sdk::rpc_types::{
    SuiObjectDataFilter, SuiObjectDataOptions, SuiObjectResponseQuery, SuiRawData,
};
use sui_sdk::types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_sdk::types::gas_coin::GasCoin;
use sui_sdk::SuiClient;

pub const DEFAULT_GAS_BUDGET: u64 = 10_000_000;

pub struct GasRet {
    pub object: ObjectRef,
    pub budget: u64,
    pub price: u64,
}

pub async fn select_gas(
    client: &SuiClient,
    signer_addr: SuiAddress,
    input_gas: Option<ObjectID>,
    budget: Option<u64>,
    exclude_objects: Vec<ObjectID>,
    gas_price: Option<u64>,
) -> Result<GasRet> {
    let price = match gas_price {
        Some(p) => p,
        None => {
            debug!("No gas price given, fetching from fullnode");
            client.read_api().get_reference_gas_price().await?
        }
    };
    let budget = budget.unwrap_or_else(|| {
        debug!("No gas budget given, defaulting to {DEFAULT_GAS_BUDGET}");
        debug_assert!(DEFAULT_GAS_BUDGET > price);
        DEFAULT_GAS_BUDGET
    });
    if budget < price {
        return Err(anyhow!(
            "Gas budget {budget} is less than the reference gas price {price}.
              The gas budget must be at least the current reference gas price of {price}."
        ));
    }

    if let Some(gas) = input_gas {
        let read_api = client.read_api();
        let object = read_api
            .get_object_with_options(gas, SuiObjectDataOptions::new())
            .await?
            .object_ref_if_exists()
            .ok_or(anyhow!("No object-ref"))?;
        return Ok(GasRet {
            object,
            budget,
            price,
        });
    }

    let read_api = client.read_api();
    let gas_objs = read_api
        .get_owned_objects(
            signer_addr,
            Some(SuiObjectResponseQuery {
                filter: Some(SuiObjectDataFilter::StructType(GasCoin::type_())),
                options: Some(SuiObjectDataOptions::new().with_bcs()),
            }),
            None,
            None,
        )
        .await?
        .data;

    for obj in gas_objs {
        let SuiRawData::MoveObject(raw_obj) = &obj
            .data
            .as_ref()
            .ok_or_else(|| anyhow!("data field is unexpectedly empty"))?
            .bcs
            .as_ref()
            .ok_or_else(|| anyhow!("bcs field is unexpectedly empty"))?
        else {
            continue;
        };

        let gas: GasCoin = bcs::from_bytes(&raw_obj.bcs_bytes)?;

        let Some(obj_ref) = obj.object_ref_if_exists() else {
            continue;
        };
        if !exclude_objects.contains(&obj_ref.0) && gas.value() >= budget {
            return Ok(GasRet {
                object: obj_ref,
                budget,
                price,
            });
        }
    }
    Err(anyhow!("Cannot find gas coin for signer address [{signer_addr}] with amount sufficient for the required gas amount [{budget}]."))
}
