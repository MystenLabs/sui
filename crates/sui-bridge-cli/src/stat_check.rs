// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// https://ethereum-sepolia.blockpi.network/v1/rpc/public 

use ethers::abi::AbiEncode;
use ethers::providers::Middleware;
use ethers::types::Address as EthAddress;
use ethers::types::H256;
use ethers::types::U256;
use move_core_types::ident_str;
use sui_types::event::EventID;
use tokio::time::sleep;
use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use sui_bridge::abi::EthSuiBridgeEvents;
use sui_bridge::abi::{EthBridgeEvent, TokensClaimedFilter, TokensDepositedFilter};
use sui_bridge::eth_client::EthClient;
use sui_bridge::events::SuiBridgeEvent;
use sui_bridge::events::{EmittedSuiToEthTokenBridgeV1, TokenTransferClaimed};
use sui_bridge::sui_client::SuiClient;
use sui_types::base_types::SuiAddress;
use sui_types::bridge::BridgeChainId;
use sui_types::digests::TransactionDigest;
use sui_types::BRIDGE_PACKAGE_ID;
use tokio::time::Instant;
use std::time::{SystemTime, UNIX_EPOCH};

const ETH_START_BLOCK: u64 = 5997013; // contract creation block
const BRIDGE_PROXY: &str = "0xAE68F87938439afEEDd6552B0E83D2CbC2473623";

pub async fn foo(eth_rpc_url: String, sui_rpc_url: String) {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let eth_task = tokio::spawn(query_eth(eth_rpc_url));
    let sui_task = tokio::spawn(query_sui(sui_rpc_url));
    let (eth_deposits, eth_claims) = eth_task.await.unwrap();
    let (sui_deposits, sui_claims) = sui_task.await.unwrap();
    let file_name_eth_tx = format!("bridge_stat_{}.eth_tx.csv", since_the_epoch);
    let file_name_sui_tx = format!("bridge_stat_{}.sui_tx.csv", since_the_epoch);
    let file_name_eth_addr  = format!("bridge_stat_{}.eth_addr.csv", since_the_epoch);
    let file_name_sui_addr = format!("bridge_stat_{}.sui_addr.csv", since_the_epoch);
    println!("Writing to file {file_name_eth_tx} & {file_name_sui_tx}");

    let mut sui_address_sent_and_received: HashMap<SuiAddress, (u64, u64)> = HashMap::new();
    let mut eth_address_sent_and_received: HashMap<EthAddress, (u64, u64)> = HashMap::new();

    let mut wtr = csv::Writer::from_path(file_name_eth_tx).unwrap();
    for (nonce, deposit) in eth_deposits {
        let (_sui_sent, sui_received) = sui_address_sent_and_received
            .entry(deposit.recipient_address)
            .or_insert((0, 0));
        *sui_received += 1;
        let (eth_sent, _eth_received) = eth_address_sent_and_received
            .entry(deposit.sender_address)
            .or_insert((0, 0));
        *eth_sent += 1;
        let sui_claim = sui_claims.get(&nonce);
        wtr.write_record(&[
            nonce.to_string(),
            format!("0x{:x}", deposit.sender_address),
            // deposit.sender_address.encode_hex(),
            deposit.recipient_address.to_string(),
            deposit.token_id.to_string(),
            deposit.sui_adjusted_amount.to_string(),
            deposit.transaction_hash.encode_hex(),
            sui_claim
                .map(|c| c.tx_digest.to_string())
                .unwrap_or_default(),
        ])
        .unwrap();
    }
    wtr.flush().unwrap();

    let mut wtr = csv::Writer::from_path(file_name_sui_tx).unwrap();
    for (nonce, deposit) in sui_deposits {
        let (sui_sent, _sui_received) = sui_address_sent_and_received
            .entry(deposit.sender_address)
            .or_insert((0, 0));
        *sui_sent += 1;
        let (_eth_sent, eth_received) = eth_address_sent_and_received
            .entry(deposit.recipient_address)
            .or_insert((0, 0));
        *eth_received += 1;
        let eth_claim = eth_claims.get(&nonce);
        wtr.write_record(&[
            nonce.to_string(),
            deposit.sender_address.to_string(),
            format!("0x{:x}", deposit.recipient_address),
            // deposit.recipient_address.encode_hex(),
            deposit.token_id.to_string(),
            deposit.sui_adjusted_amount.to_string(),
            deposit.tx_digest.to_string(),
            eth_claim
                .map(|c| c.transaction_hash.encode_hex())
                .unwrap_or_default(),
        ])
        .unwrap();
    }
    wtr.flush().unwrap();

    let mut wtr = csv::Writer::from_path(file_name_sui_addr).unwrap();
    for (addr, (sent, received)) in sui_address_sent_and_received {
        wtr.write_record(&[
            addr.to_string(),
            sent.to_string(),
            received.to_string(),
        ])
        .unwrap();
    }
    wtr.flush().unwrap();
}

async fn query_sui(sui_rpc_url: String) -> (BTreeMap<u64, SuiDeposit>, BTreeMap<u64, SuiClaim>) {
    let sui_bridge_client = SuiClient::new(&sui_rpc_url).await.unwrap();
    let mut cursor = None;
    // let mut cursor = Some(EventID {
    //     // tx_digest: TransactionDigest::from_str("GMM4mA9CPPVQY2eTAaxhiX3wqS4aHXQKcAqgfucvMeoH").unwrap(),
    //     tx_digest: TransactionDigest::from_str("DCqBC9N1RUxj72hNj2ZbysHmu7pX6fGpwsYh2yS6Upuu").unwrap(),
    //     event_seq: 0,
    // });
    let mut all_deposits = BTreeMap::new();
    let mut all_claims = BTreeMap::new();
    let timer = Instant::now();
    let mut print_counter = 0;
    loop {
        if print_counter % 50 == 0 {
            println!("querying sui from {:?}", cursor);
        }
        let sui_events = match sui_bridge_client
            .query_events_by_module(BRIDGE_PACKAGE_ID, ident_str!("bridge").to_owned(), cursor)
            .await {
            Ok(sui_events) => sui_events,
            Err(e) => {
                println!("ERROR: {:?}. Sleeping for 5s", e);
                sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };
        cursor = sui_events.next_cursor;
        let has_next_page = sui_events.has_next_page;
        let sui_events = sui_events.data;
        for e in sui_events {
            match SuiBridgeEvent::try_from_sui_event(&e) {
                Err(e) => {
                    println!("ERROR: {:?}", e);
                }
                Ok(Some(SuiBridgeEvent::SuiToEthTokenBridgeV1(EmittedSuiToEthTokenBridgeV1 {
                    sui_chain_id,
                    eth_chain_id,
                    sui_address,
                    eth_address,
                    amount_sui_adjusted,
                    token_id,
                    nonce,
                }))) => {
                    assert_eq!(sui_chain_id, BridgeChainId::SuiTestnet);
                    assert_eq!(eth_chain_id, BridgeChainId::EthSepolia);
                    all_deposits.insert(
                        nonce,
                        SuiDeposit {
                            token_id,
                            sender_address: sui_address,
                            recipient_address: eth_address,
                            sui_adjusted_amount: amount_sui_adjusted,
                            nonce,
                            tx_digest: e.id.tx_digest,
                        },
                    );
                }
                Ok(Some(SuiBridgeEvent::TokenTransferClaimed(TokenTransferClaimed {
                    nonce,
                    source_chain,
                }))) => {
                    assert_eq!(source_chain, BridgeChainId::EthSepolia);
                    all_claims.insert(
                        nonce,
                        SuiClaim {
                            nonce,
                            tx_digest: e.id.tx_digest,
                        },
                    );
                }
                _ => (),
            }
        }
        print_counter += 1;
        if !has_next_page {
            break;
        }
    }
    println!("query sui took: {:?}", timer.elapsed());
    (all_deposits, all_claims)
}

async fn query_eth(eth_rpc_url: String) -> (BTreeMap<u64, EthDeposit>, BTreeMap<u64, EthClaim>) {
    let timer = Instant::now();
    let provider = Arc::new(
        ethers::prelude::Provider::<ethers::providers::Http>::try_from(eth_rpc_url.clone())
            .unwrap()
            .interval(std::time::Duration::from_millis(2000)),
    );
    let latest_block_num = provider.get_block_number().await.unwrap().as_u64();
    let eth_client = EthClient::new(
        &eth_rpc_url,
        HashSet::from_iter(vec![EthAddress::from_str(BRIDGE_PROXY).unwrap()]),
    )
    .await
    .unwrap();
    let mut start_block = ETH_START_BLOCK;
    let mut all_claims = BTreeMap::new();
    let mut all_deposits = BTreeMap::new();
    loop {
        if start_block > latest_block_num {
            break;
        }
        let end_block = std::cmp::min(start_block + 1000, latest_block_num);
        println!("querying eth from {} to {}", start_block, end_block);
        let logs = match eth_client
            .get_events_in_range(
                EthAddress::from_str(BRIDGE_PROXY).unwrap(),
                start_block,
                end_block,
            )
            .await {
            Ok(logs) => logs,
            Err(e) => {
                println!("ERROR: {:?}. Sleeping for 30s", e);
                sleep(tokio::time::Duration::from_secs(30)).await;
                continue;
            }
        };
        for log in logs {
            match EthBridgeEvent::try_from_eth_log(&log) {
                Some(EthBridgeEvent::EthSuiBridgeEvents(
                    EthSuiBridgeEvents::TokensClaimedFilter(filter),
                )) => {
                    let TokensClaimedFilter {
                        source_chain_id,
                        erc_20_adjusted_amount,
                        sender_address,
                        token_id,
                        recipient_address,
                        nonce,
                        destination_chain_id,
                    } = filter;
                    assert_eq!(source_chain_id, BridgeChainId::SuiTestnet as u8);
                    assert_eq!(destination_chain_id, BridgeChainId::EthSepolia as u8);
                    all_claims.insert(
                        nonce,
                        EthClaim {
                            erc_20_adjusted_amount,
                            sender_address: SuiAddress::from_bytes(&sender_address.0).unwrap(),
                            token_id,
                            recipient_address,
                            nonce,
                            transaction_hash: log.tx_hash,
                        },
                    );
                }
                Some(EthBridgeEvent::EthSuiBridgeEvents(
                    EthSuiBridgeEvents::TokensDepositedFilter(filter),
                )) => {
                    let TokensDepositedFilter {
                        source_chain_id,
                        sui_adjusted_amount,
                        sender_address,
                        token_id,
                        recipient_address,
                        nonce,
                        destination_chain_id,
                    } = filter;
                    assert_eq!(source_chain_id, BridgeChainId::EthSepolia as u8);
                    assert_eq!(destination_chain_id, BridgeChainId::SuiTestnet as u8);
                    all_deposits.insert(
                        nonce,
                        EthDeposit {
                            sui_adjusted_amount,
                            sender_address,
                            token_id,
                            recipient_address: SuiAddress::from_bytes(&recipient_address.0)
                                .unwrap(),
                            nonce,
                            transaction_hash: log.tx_hash,
                        },
                    );
                }
                _ => (),
            };
        }
        start_block = end_block + 1;
    }
    println!("query eth took: {:?}", timer.elapsed());
    (all_deposits, all_claims)
}

// enum EthOp {
//     Claim(EthClaim),
//     Deposit(EthDeposit),
// }

#[derive(Debug)]
struct EthClaim {
    erc_20_adjusted_amount: U256,
    sender_address: SuiAddress,
    token_id: u8,
    recipient_address: EthAddress,
    nonce: u64,
    transaction_hash: H256,
}

#[derive(Debug)]
struct EthDeposit {
    sui_adjusted_amount: u64,
    sender_address: EthAddress,
    token_id: u8,
    recipient_address: SuiAddress,
    nonce: u64,
    transaction_hash: H256,
}

// enum SuiOp {
//     Claim(SuiClaim),
//     Deposit(SuiDeposit),
// }
#[derive(Debug)]
struct SuiDeposit {
    token_id: u8,
    sender_address: SuiAddress,
    recipient_address: EthAddress,
    sui_adjusted_amount: u64,
    nonce: u64,
    tx_digest: TransactionDigest,
}

#[derive(Debug)]
struct SuiClaim {
    nonce: u64,
    tx_digest: TransactionDigest,
}
