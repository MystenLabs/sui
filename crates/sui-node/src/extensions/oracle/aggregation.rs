use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use super::{MedianPrice, PuiPriceStorage};

pub fn aggregate_to_median(
    storages: Vec<PuiPriceStorage>,
    checkpoint: CheckpointSequenceNumber,
) -> Option<MedianPrice> {
    let mut prices: Vec<u128> = storages
        .iter()
        .filter_map(|storage| storage.price)
        .collect();

    if prices.is_empty() {
        return None;
    }

    let latest_timestamp = storages
        .iter()
        .filter_map(|storage| storage.timestamp)
        .max();

    let median_price = {
        prices.sort_unstable();
        let mid = prices.len() / 2;
        if prices.len() % 2 == 0 {
            Some((prices[mid - 1] + prices[mid]) / 2)
        } else {
            Some(prices[mid])
        }
    };

    Some(MedianPrice {
        pair: "BTC/USD".to_string(),
        median_price,
        timestamp: latest_timestamp,
        checkpoint: Some(checkpoint),
    })
}
