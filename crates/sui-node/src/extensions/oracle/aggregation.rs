use super::{AggregatedPrice, PuiPriceStorage};

pub fn calculate_aggregated_price(storages: &[PuiPriceStorage]) -> AggregatedPrice {
    let mut prices: Vec<u128> = storages
        .iter()
        .filter_map(|storage| storage.price)
        .collect();

    let latest_timestamp = storages
        .iter()
        .filter_map(|storage| storage.timestamp)
        .max();

    let median_price = if !prices.is_empty() {
        prices.sort_unstable();
        let mid = prices.len() / 2;
        if prices.len() % 2 == 0 {
            Some((prices[mid - 1] + prices[mid]) / 2)
        } else {
            Some(prices[mid])
        }
    } else {
        None
    };

    AggregatedPrice {
        pair: "BTC/USD".to_string(),
        median_price,
        timestamp: latest_timestamp,
    }
}
