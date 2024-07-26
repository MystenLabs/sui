// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `BridgeMonitor` receives all `SuiBridgeEvent` and handles them accordingly.

use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::time::Duration;

use crate::client::bridge_authority_aggregator::BridgeAuthorityAggregator;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::events::CommitteeMemberUrlUpdateEvent;
use crate::events::SuiBridgeEvent;
use crate::retry_with_max_elapsed_time;
use crate::sui_client::{SuiClient, SuiClientInner};
use crate::types::BridgeCommittee;
use tracing::{error, info, warn};

const REFRESH_COMMITTEE_RETRY_TIMES: u64 = 3;

pub struct BridgeMonitor<C> {
    sui_client: Arc<SuiClient<C>>,
    monitor_rx: mysten_metrics::metered_channel::Receiver<SuiBridgeEvent>,
    bridge_auth_agg: Arc<ArcSwap<BridgeAuthorityAggregator>>,
}

impl<C> BridgeMonitor<C>
where
    C: SuiClientInner + 'static,
{
    pub fn new(
        sui_client: Arc<SuiClient<C>>,
        monitor_rx: mysten_metrics::metered_channel::Receiver<SuiBridgeEvent>,
        bridge_auth_agg: Arc<ArcSwap<BridgeAuthorityAggregator>>,
    ) -> Self {
        Self {
            sui_client,
            monitor_rx,
            bridge_auth_agg,
        }
    }

    pub async fn run(self) {
        tracing::info!("Starting BridgeMonitor");
        let Self {
            sui_client,
            mut monitor_rx,
            bridge_auth_agg,
        } = self;

        while let Some(events) = monitor_rx.recv().await {
            match events {
                SuiBridgeEvent::SuiToEthTokenBridgeV1(_) => (),
                SuiBridgeEvent::TokenTransferApproved(_) => (),
                SuiBridgeEvent::TokenTransferClaimed(_) => (),
                SuiBridgeEvent::TokenTransferAlreadyApproved(_) => (),
                SuiBridgeEvent::TokenTransferAlreadyClaimed(_) => (),
                SuiBridgeEvent::TokenTransferLimitExceed(_) => {
                    // TODO
                }
                SuiBridgeEvent::EmergencyOpEvent(_) => {
                    // TODO
                }
                SuiBridgeEvent::CommitteeMemberRegistration(_) => (),
                SuiBridgeEvent::CommitteeUpdateEvent(_) => (),
                SuiBridgeEvent::CommitteeMemberUrlUpdateEvent(event) => {
                    info!("Received CommitteeMemberUrlUpdateEvent: {:?}", event);
                    let new_committee = get_latest_bridge_committee_with_url_update_event(
                        sui_client.clone(),
                        event,
                        Duration::from_secs(10),
                    )
                    .await;
                    bridge_auth_agg.store(Arc::new(BridgeAuthorityAggregator::new(Arc::new(
                        new_committee,
                    ))));
                    info!("Committee updated");
                }
                SuiBridgeEvent::BlocklistValidatorEvent(_) => {
                    // TODO
                }
                SuiBridgeEvent::TokenRegistrationEvent(_) => (),
                SuiBridgeEvent::NewTokenEvent(_) => {
                    // TODO
                }
                SuiBridgeEvent::UpdateTokenPriceEvent(_) => (),
            }
        }

        panic!("BridgeMonitor channel was closed unexpectedly");
    }
}

async fn get_latest_bridge_committee_with_url_update_event<C: SuiClientInner>(
    sui_client: Arc<SuiClient<C>>,
    event: CommitteeMemberUrlUpdateEvent,
    staleness_retry_interval: Duration,
) -> BridgeCommittee {
    let mut remaining_retry_times = REFRESH_COMMITTEE_RETRY_TIMES;
    loop {
        let Ok(Ok(committee)) = retry_with_max_elapsed_time!(
            sui_client.get_bridge_committee(),
            Duration::from_secs(600)
        ) else {
            error!("Failed to get bridge committee after retry");
            continue;
        };
        let member = committee.member(&BridgeAuthorityPublicKeyBytes::from(&event.member));
        let Some(member) = member else {
            // This is possible when a node is processing an older event while the member quitted at a later point, which is fine.
            // Or fullnode returns a stale committee that the member hasn't joined, which is rare and tricy to handle so we just log it.
            warn!(
                "Committee member not found in the committee: {:?}",
                event.member
            );
            return committee;
        };
        if member.base_url == event.new_url {
            return committee;
        }
        // If url does not match, it could be:
        // 1. the query is sent to a stale fullnode that does not have the latest data yet
        // 2. the node is processing an older message, and the latest url has changed again
        // In either case, we retry a few times. If it still fails to match, we assume it's the latter case.
        tokio::time::sleep(staleness_retry_interval).await;
        remaining_retry_times -= 1;
        if remaining_retry_times == 0 {
            warn!(
                "Committee member url {:?} does not match onchain record {:?} after retry",
                event.member, member
            );
            return committee;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::init_all_struct_tags;
    use crate::test_utils::{
        bridge_committee_to_bridge_committee_summary, get_test_authority_and_key,
    };
    use fastcrypto::traits::KeyPair;
    use prometheus::Registry;
    use sui_types::base_types::SuiAddress;
    use sui_types::bridge::BridgeCommitteeSummary;
    use sui_types::bridge::MoveTypeCommitteeMember;
    use sui_types::crypto::get_key_pair;

    use crate::{sui_mock_client::SuiMockClient, types::BridgeCommittee};
    use sui_types::crypto::ToFromBytes;

    #[tokio::test]
    async fn test_get_latest_bridge_committee_with_url_update_event() {
        telemetry_subscribers::init_for_testing();
        let sui_client_mock = SuiMockClient::default();
        let sui_client = Arc::new(SuiClient::new_for_testing(sui_client_mock.clone()));
        let (_, kp): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
        let pk = kp.public().clone();
        let pk_as_bytes = BridgeAuthorityPublicKeyBytes::from(&pk);
        let pk_bytes = pk_as_bytes.as_bytes().to_vec();
        let event = CommitteeMemberUrlUpdateEvent {
            member: pk,
            new_url: "http://new.url".to_string(),
        };
        let summary = BridgeCommitteeSummary {
            members: vec![(
                pk_bytes.clone(),
                MoveTypeCommitteeMember {
                    sui_address: SuiAddress::random_for_testing_only(),
                    bridge_pubkey_bytes: pk_bytes.clone(),
                    voting_power: 10000,
                    http_rest_url: "http://new.url".to_string().as_bytes().to_vec(),
                    blocklisted: false,
                },
            )],
            member_registration: vec![],
            last_committee_update_epoch: 0,
        };

        // Test the regular case, the onchain url matches
        sui_client_mock.set_bridge_committee(summary.clone());
        let timer = std::time::Instant::now();
        let committee = get_latest_bridge_committee_with_url_update_event(
            sui_client.clone(),
            event.clone(),
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(
            committee.member(&pk_as_bytes).unwrap().base_url,
            "http://new.url"
        );
        assert!(timer.elapsed().as_millis() < 500);

        // Test the case where the onchain url is older. Then update onchain url in 1 second.
        // Since the retry interval is 2 seconds, it should return the next retry.
        let old_summary = BridgeCommitteeSummary {
            members: vec![(
                pk_bytes.clone(),
                MoveTypeCommitteeMember {
                    sui_address: SuiAddress::random_for_testing_only(),
                    bridge_pubkey_bytes: pk_bytes.clone(),
                    voting_power: 10000,
                    http_rest_url: "http://old.url".to_string().as_bytes().to_vec(),
                    blocklisted: false,
                },
            )],
            member_registration: vec![],
            last_committee_update_epoch: 0,
        };
        sui_client_mock.set_bridge_committee(old_summary.clone());
        let timer = std::time::Instant::now();
        // update the url to "http://new.url" in 1 second
        let sui_client_mock_clone = sui_client_mock.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(1)).await;
            sui_client_mock_clone.set_bridge_committee(summary.clone());
        });
        let committee = get_latest_bridge_committee_with_url_update_event(
            sui_client.clone(),
            event.clone(),
            Duration::from_secs(2),
        )
        .await;
        assert_eq!(
            committee.member(&pk_as_bytes).unwrap().base_url,
            "http://new.url"
        );
        let elapsed = timer.elapsed().as_millis();
        assert!(elapsed > 1000 && elapsed < 3000);

        // Test the case where the onchain url is newer. It should retry up to
        // REFRESH_COMMITTEE_RETRY_TIMES time then return the onchain record.
        let newer_summary = BridgeCommitteeSummary {
            members: vec![(
                pk_bytes.clone(),
                MoveTypeCommitteeMember {
                    sui_address: SuiAddress::random_for_testing_only(),
                    bridge_pubkey_bytes: pk_bytes.clone(),
                    voting_power: 10000,
                    http_rest_url: "http://newer.url".to_string().as_bytes().to_vec(),
                    blocklisted: false,
                },
            )],
            member_registration: vec![],
            last_committee_update_epoch: 0,
        };
        sui_client_mock.set_bridge_committee(newer_summary.clone());
        let timer = std::time::Instant::now();
        let committee = get_latest_bridge_committee_with_url_update_event(
            sui_client.clone(),
            event.clone(),
            Duration::from_millis(500),
        )
        .await;
        assert_eq!(
            committee.member(&pk_as_bytes).unwrap().base_url,
            "http://newer.url"
        );
        let elapsed = timer.elapsed().as_millis();
        assert!(elapsed > 500 * REFRESH_COMMITTEE_RETRY_TIMES as u128);

        // Test the case where the member is not found in the committee
        // It should return the onchain record.
        let (_, kp2): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
        let pk2 = kp2.public().clone();
        let pk_as_bytes2 = BridgeAuthorityPublicKeyBytes::from(&pk2);
        let pk_bytes2 = pk_as_bytes2.as_bytes().to_vec();
        let newer_summary = BridgeCommitteeSummary {
            members: vec![(
                pk_bytes2.clone(),
                MoveTypeCommitteeMember {
                    sui_address: SuiAddress::random_for_testing_only(),
                    bridge_pubkey_bytes: pk_bytes2.clone(),
                    voting_power: 10000,
                    http_rest_url: "http://newer.url".to_string().as_bytes().to_vec(),
                    blocklisted: false,
                },
            )],
            member_registration: vec![],
            last_committee_update_epoch: 0,
        };
        sui_client_mock.set_bridge_committee(newer_summary.clone());
        let timer = std::time::Instant::now();
        let committee = get_latest_bridge_committee_with_url_update_event(
            sui_client.clone(),
            event.clone(),
            Duration::from_secs(1),
        )
        .await;
        assert_eq!(
            committee.member(&pk_as_bytes2).unwrap().base_url,
            "http://newer.url"
        );
        assert!(committee.member(&pk_as_bytes).is_none());
        let elapsed = timer.elapsed().as_millis();
        assert!(elapsed < 1000);
    }

    #[tokio::test]
    async fn test_update_bridge_authority_aggregation_with_url_change_event() {
        let (monitor_tx, monitor_rx, sui_client_mock, sui_client) = setup();
        let mut authorities = vec![
            get_test_authority_and_key(2500, 0 /* port, dummy value */).0,
            get_test_authority_and_key(2500, 0 /* port, dummy value */).0,
            get_test_authority_and_key(2500, 0 /* port, dummy value */).0,
            get_test_authority_and_key(2500, 0 /* port, dummy value */).0,
        ];
        let old_committee = BridgeCommittee::new(authorities.clone()).unwrap();
        let agg = Arc::new(ArcSwap::new(Arc::new(BridgeAuthorityAggregator::new(
            Arc::new(old_committee),
        ))));
        let _handle = tokio::task::spawn(
            BridgeMonitor::new(sui_client.clone(), monitor_rx, agg.clone()).run(),
        );
        let new_url = "http://new.url".to_string();
        authorities[0].base_url = new_url.clone();
        let new_committee = BridgeCommittee::new(authorities.clone()).unwrap();
        let new_committee_summary =
            bridge_committee_to_bridge_committee_summary(new_committee.clone());
        sui_client_mock.set_bridge_committee(new_committee_summary.clone());
        monitor_tx
            .send(SuiBridgeEvent::CommitteeMemberUrlUpdateEvent(
                CommitteeMemberUrlUpdateEvent {
                    member: authorities[0].pubkey.clone(),
                    new_url: new_url.clone(),
                },
            ))
            .await
            .unwrap();
        // Wait for the monitor to process the event
        tokio::time::sleep(Duration::from_secs(1)).await;
        // Now expect the committee to be updated
        assert_eq!(
            agg.load()
                .committee
                .member(&BridgeAuthorityPublicKeyBytes::from(&authorities[0].pubkey))
                .unwrap()
                .base_url,
            new_url
        );
    }

    fn setup() -> (
        mysten_metrics::metered_channel::Sender<SuiBridgeEvent>,
        mysten_metrics::metered_channel::Receiver<SuiBridgeEvent>,
        SuiMockClient,
        Arc<SuiClient<SuiMockClient>>,
    ) {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        init_all_struct_tags();

        let sui_client_mock = SuiMockClient::default();
        let sui_client = Arc::new(SuiClient::new_for_testing(sui_client_mock.clone()));
        let (monitor_tx, monitor_rx) = mysten_metrics::metered_channel::channel(
            10000,
            &mysten_metrics::get_metrics()
                .unwrap()
                .channel_inflight
                .with_label_values(&["monitor_queue"]),
        );
        (monitor_tx, monitor_rx, sui_client_mock, sui_client)
    }
}
