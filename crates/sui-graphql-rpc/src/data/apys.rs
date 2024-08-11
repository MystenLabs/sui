// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use itertools::Itertools;
use sui_types::sui_system_state::PoolTokenExchangeRate;

/// Calculate an APY for a validator based on the exchange rates of the staking pool.
///
/// This is copied from the previous sui-json-rpc/governance_api crate, together with tests, and
/// slightly altered to return one APY for each call instead of multiple ones.
///
/// See original code here:
/// <https://github.com/MystenLabs/sui/blob/c3feec3ac3b626bf2fd40c668ba32be9c73e7528/crates/sui-json-rpc/src/governance_api.rs#L280>
pub(crate) fn calculate_apy(
    stake_subsidy_start_epoch: u64,
    rates: &[(u64, PoolTokenExchangeRate)],
) -> f64 {
    // we start the apy calculation from the epoch when the stake subsidy starts
    let exchange_rates = rates
        .iter()
        .filter_map(|(epoch, rate)| {
            if epoch >= &stake_subsidy_start_epoch {
                Some(rate)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let exchange_rates_size = exchange_rates.len();

    // we need at least 2 data points to calculate apy
    if exchange_rates_size >= 2 {
        // rates are sorted by epoch in descending order.
        let er_e = exchange_rates.iter().dropping(1);
        // rate e+1
        let er_e_1 = exchange_rates.iter().dropping_back(1);
        let apys = er_e
            .zip(er_e_1)
            .map(apy_rate)
            .filter(|apy| *apy > 0.0 && *apy < 0.1)
            .take(30)
            .collect::<Vec<_>>();

        let apy_counts = apys.len() as f64;
        apys.iter().sum::<f64>() / apy_counts
    } else {
        0.0
    }
}

// APY_e = (ER_e+1 / ER_e) ^ 365
pub(crate) fn apy_rate(
    (rate_e, rate_e_1): (&&PoolTokenExchangeRate, &&PoolTokenExchangeRate),
) -> f64 {
    (rate_e.rate() / rate_e_1.rate()).powf(365.0) - 1.0
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sui_json_rpc::governance_api::ValidatorExchangeRates;
    use sui_types::base_types::{ObjectID, SuiAddress};

    use super::*;

    #[test]
    fn test_apys_calculation_filter_outliers() {
        // staking pool exchange rates extracted from mainnet
        let file =
            std::fs::File::open("src/unit_tests_data/validator_exchange_rates.json").unwrap();
        let rates: BTreeMap<String, Vec<(u64, PoolTokenExchangeRate)>> =
            serde_json::from_reader(file).unwrap();

        let mut validator_exchange_rates = BTreeMap::new();
        rates.into_iter().for_each(|(validator, rates)| {
            let address = SuiAddress::random_for_testing_only();
            validator_exchange_rates.insert(
                address,
                (
                    validator,
                    ValidatorExchangeRates {
                        address,
                        pool_id: ObjectID::random(),
                        active: true,
                        rates,
                    },
                ),
            );
        });

        for (address, (validator, rates)) in &validator_exchange_rates {
            let apy = calculate_apy(20, &rates.rates);
            println!("{} {}: {}", validator, address, apy);
            assert!(apy < 0.07)
        }
    }
}
