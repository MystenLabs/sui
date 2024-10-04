// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::types::{
    AccountBalanceRequest, Amount, ConstructionMetadata, Currency, CurrencyMetadata,
};
use quick_js::Context;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sui_types::base_types::{ObjectRef, SuiAddress};

#[tokio::test]
async fn test_currency_defaults() {
    let expected = Currency {
        symbol: "SUI".to_string(),
        decimals: 9,
        metadata: CurrencyMetadata {
            coin_type: "0x2::sui::SUI".to_string(),
        },
    };

    let currency: Currency = serde_json::from_value(json!(
        {
            "symbol": "SUI",
            "decimals": 9,
        }
    ))
    .unwrap();
    assert_eq!(expected, currency);

    let amount: Amount = serde_json::from_value(json!(
        {
            "value": "1000000000",
        }
    ))
    .unwrap();
    assert_eq!(expected, amount.currency);

    let account_balance_request: AccountBalanceRequest = serde_json::from_value(json!(
        {
            "network_identifier": {
                "blockchain": "sui",
                "network": "mainnet"
            },
            "account_identifier": {
                "address": "0xadc3a0bb21840f732435f8b649e99df6b29cd27854dfa4b020e3bee07ea09b96"
            }
        }
    ))
    .unwrap();
    assert_eq!(
        expected,
        account_balance_request.currencies.0.clone().pop().unwrap()
    );

    let account_balance_request: AccountBalanceRequest = serde_json::from_value(json!(
        {
            "network_identifier": {
                "blockchain": "sui",
                "network": "mainnet"
            },
            "account_identifier": {
                "address": "0xadc3a0bb21840f732435f8b649e99df6b29cd27854dfa4b020e3bee07ea09b96"
            },
            "currencies": []
        }
    ))
    .unwrap();
    assert_eq!(
        expected,
        account_balance_request.currencies.0.clone().pop().unwrap()
    );
}

#[tokio::test]
async fn test_metadata_total_coin_value_js_conversion_for_large_balance() {
    #[derive(Serialize, Deserialize, Debug)]
    pub struct TestConstructionMetadata {
        pub sender: SuiAddress,
        pub coins: Vec<ObjectRef>,
        pub objects: Vec<ObjectRef>,
        pub total_coin_value: u64,
        pub gas_price: u64,
        pub budget: u64,
        pub currency: Option<Currency>,
    }

    let test_metadata = TestConstructionMetadata {
        sender: Default::default(),
        coins: vec![],
        objects: vec![],
        total_coin_value: 65_000_004_233_578_496,
        gas_price: 0,
        budget: 0,
        currency: None,
    };
    let test_metadata_json = serde_json::to_string(&test_metadata).unwrap();

    let prod_metadata = ConstructionMetadata {
        sender: Default::default(),
        coins: vec![],
        objects: vec![],
        total_coin_value: 65_000_004_233_578_496,
        gas_price: 0,
        budget: 0,
        currency: None,
    };
    let prod_metadata_json = serde_json::to_string(&prod_metadata).unwrap();

    let context = Context::new().unwrap();

    let test_total_coin_value = format!(
        "JSON.parse({:?}).total_coin_value.toString()",
        test_metadata_json
    );
    let js_test_total_coin_value = context.eval_as::<String>(&test_total_coin_value).unwrap();

    let prod_total_coin_value = format!(
        "JSON.parse({:?}).total_coin_value.toString()",
        prod_metadata_json
    );
    let js_prod_total_coin_value = context.eval_as::<String>(&prod_total_coin_value).unwrap();

    assert_eq!("65000004233578500", js_test_total_coin_value);
    assert_eq!("65000004233578496", js_prod_total_coin_value);
}
