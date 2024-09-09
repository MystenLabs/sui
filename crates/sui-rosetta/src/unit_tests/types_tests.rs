// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::types::{AccountBalanceRequest, Amount, Currency, CurrencyMetadata};
use serde_json::json;

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
