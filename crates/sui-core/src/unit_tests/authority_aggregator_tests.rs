// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::test_utils::make_transfer_sui_transaction;
use bcs::to_bytes;
use move_core_types::{account_address::AccountAddress, ident_str};
use multiaddr::Multiaddr;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use sui_framework_build::compiled_package::BuildConfig;
use sui_types::crypto::AuthoritySignature;
use sui_types::crypto::{
    get_authority_key_pair, get_key_pair, AccountKeyPair, AuthorityKeyPair, AuthorityPublicKeyBytes,
};
use sui_types::crypto::{KeypairTraits, Signature};
use test_utils::sui_system_state::{test_sui_system_state, test_validator};

use sui_macros::sim_test;
use sui_types::messages::*;
use sui_types::object::{MoveObject, Object, Owner, GAS_VALUE_FOR_TESTING};
use test_utils::messages::make_random_certified_transaction;

use super::*;
use crate::authority_client::AuthorityAPI;
use crate::test_authority_clients::{
    HandleTransactionTestAuthorityClient, LocalAuthorityClient, LocalAuthorityClientFaultConfig,
    MockAuthorityApi,
};
use crate::test_utils::init_local_authorities;
use sui_types::utils::to_sender_signed_transaction;
use tokio::time::Instant;

#[cfg(msim)]
use sui_simulator::configs::constant_latency_ms;

pub fn get_local_client(
    authorities: &mut AuthorityAggregator<LocalAuthorityClient>,
    index: usize,
) -> &mut LocalAuthorityClient {
    let mut clients = authorities.authority_clients.values_mut();
    let mut i = 0;
    while i < index {
        clients.next();
        i += 1;
    }
    clients.next().unwrap().authority_client_mut()
}

pub fn transfer_coin_transaction(
    src: SuiAddress,
    secret: &dyn signature::Signer<Signature>,
    dest: SuiAddress,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
) -> VerifiedTransaction {
    to_sender_signed_transaction(
        TransactionData::new_transfer_with_dummy_gas_price(
            dest,
            object_ref,
            src,
            gas_object_ref,
            GAS_VALUE_FOR_TESTING / 2,
        ),
        secret,
    )
}

pub fn transfer_object_move_transaction(
    src: SuiAddress,
    secret: &dyn signature::Signer<Signature>,
    dest: SuiAddress,
    object_ref: ObjectRef,
    framework_obj_id: ObjectID,
    gas_object_ref: ObjectRef,
) -> VerifiedTransaction {
    let args = vec![
        CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)),
        CallArg::Pure(bcs::to_bytes(&AccountAddress::from(dest)).unwrap()),
    ];

    to_sender_signed_transaction(
        TransactionData::new_move_call_with_dummy_gas_price(
            src,
            framework_obj_id,
            ident_str!("object_basics").to_owned(),
            ident_str!("transfer").to_owned(),
            Vec::new(),
            gas_object_ref,
            args,
            GAS_VALUE_FOR_TESTING / 2,
        ),
        secret,
    )
}

pub fn create_object_move_transaction(
    src: SuiAddress,
    secret: &dyn signature::Signer<Signature>,
    dest: SuiAddress,
    value: u64,
    package_id: ObjectID,
    gas_object_ref: ObjectRef,
) -> VerifiedTransaction {
    // When creating an object_basics object, we provide the value (u64) and address which will own the object
    let arguments = vec![
        CallArg::Pure(value.to_le_bytes().to_vec()),
        CallArg::Pure(bcs::to_bytes(&AccountAddress::from(dest)).unwrap()),
    ];

    to_sender_signed_transaction(
        TransactionData::new_move_call_with_dummy_gas_price(
            src,
            package_id,
            ident_str!("object_basics").to_owned(),
            ident_str!("create").to_owned(),
            Vec::new(),
            gas_object_ref,
            arguments,
            GAS_VALUE_FOR_TESTING / 2,
        ),
        secret,
    )
}

pub fn delete_object_move_transaction(
    src: SuiAddress,
    secret: &dyn signature::Signer<Signature>,
    object_ref: ObjectRef,
    framework_obj_id: ObjectID,
    gas_object_ref: ObjectRef,
) -> VerifiedTransaction {
    to_sender_signed_transaction(
        TransactionData::new_move_call_with_dummy_gas_price(
            src,
            framework_obj_id,
            ident_str!("object_basics").to_owned(),
            ident_str!("delete").to_owned(),
            Vec::new(),
            gas_object_ref,
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref))],
            GAS_VALUE_FOR_TESTING / 2,
        ),
        secret,
    )
}

pub fn set_object_move_transaction(
    src: SuiAddress,
    secret: &dyn signature::Signer<Signature>,
    object_ref: ObjectRef,
    value: u64,
    framework_obj_id: ObjectID,
    gas_object_ref: ObjectRef,
) -> VerifiedTransaction {
    let args = vec![
        CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)),
        CallArg::Pure(bcs::to_bytes(&value).unwrap()),
    ];

    to_sender_signed_transaction(
        TransactionData::new_move_call_with_dummy_gas_price(
            src,
            framework_obj_id,
            ident_str!("object_basics").to_owned(),
            ident_str!("set_value").to_owned(),
            Vec::new(),
            gas_object_ref,
            args,
            GAS_VALUE_FOR_TESTING / 2,
        ),
        secret,
    )
}

pub async fn do_transaction<A>(authority: &SafeClient<A>, transaction: &VerifiedTransaction)
where
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    authority
        .handle_transaction(transaction.clone())
        .await
        .unwrap();
}

pub async fn extract_cert<A>(
    authorities: &[&SafeClient<A>],
    committee: &Committee,
    transaction_digest: &TransactionDigest,
) -> CertifiedTransaction
where
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    let mut votes = vec![];
    let mut transaction: Option<VerifiedSignedTransaction> = None;
    for authority in authorities {
        let response = authority
            .handle_transaction_info_request(TransactionInfoRequest {
                transaction_digest: *transaction_digest,
            })
            .await;
        match response {
            Ok(VerifiedTransactionInfoResponse::Signed(signed)) => {
                votes.push(signed.auth_sig().clone());
                if let Some(inner_transaction) = transaction {
                    assert_eq!(
                        inner_transaction.data().intent_message.value,
                        signed.data().intent_message.value
                    );
                }
                transaction = Some(signed);
            }
            Ok(VerifiedTransactionInfoResponse::Executed(cert, _)) => {
                return cert.into_inner();
            }
            _ => {}
        }
    }

    CertifiedTransaction::new(transaction.unwrap().into_message(), votes, committee).unwrap()
}

pub async fn do_cert<A>(
    authority: &SafeClient<A>,
    cert: &CertifiedTransaction,
) -> TransactionEffects
where
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    authority
        .handle_certificate(cert.clone())
        .await
        .unwrap()
        .signed_effects
        .into_message()
}

pub async fn do_cert_configurable<A>(authority: &A, cert: &CertifiedTransaction)
where
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    let result = authority.handle_certificate(cert.clone()).await;
    if result.is_err() {
        println!("Error in do cert {:?}", result.err());
    }
}

pub async fn get_latest_ref<A>(authority: &SafeClient<A>, object_id: ObjectID) -> ObjectRef
where
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    if let Ok(ObjectInfoResponse {
        requested_object_reference: Some(object_ref),
        ..
    }) = authority
        .handle_object_info_request(
            ObjectInfoRequest::latest_object_info_request(object_id, None),
            false,
        )
        .await
    {
        return object_ref;
    }
    panic!("Object not found!");
}

/// Returns false if errs out, true if succeed
async fn execute_transaction_with_fault_configs(
    configs_before_process_transaction: &[(usize, LocalAuthorityClientFaultConfig)],
    configs_before_process_certificate: &[(usize, LocalAuthorityClientFaultConfig)],
) -> bool {
    let (addr1, key1): (_, AccountKeyPair) = get_key_pair();
    let (addr2, _): (_, AccountKeyPair) = get_key_pair();
    let gas_object1 = Object::with_owner_for_testing(addr1);
    let gas_object2 = Object::with_owner_for_testing(addr1);
    let (mut authorities, _, genesis, _) =
        init_local_authorities(4, vec![gas_object1.clone(), gas_object2.clone()]).await;
    let gas_object1 = genesis.object(gas_object1.id()).unwrap();
    let gas_object2 = genesis.object(gas_object2.id()).unwrap();

    for (index, config) in configs_before_process_transaction {
        get_local_client(&mut authorities, *index).fault_config = *config;
    }

    let tx = transfer_coin_transaction(
        addr1,
        &key1,
        addr2,
        gas_object1.compute_object_reference(),
        gas_object2.compute_object_reference(),
    );
    let Ok(cert) = authorities.process_transaction(tx).await else {
        return false;
    };

    for client in authorities.authority_clients.values_mut() {
        client.authority_client_mut().fault_config.reset();
    }
    for (index, config) in configs_before_process_certificate {
        get_local_client(&mut authorities, *index).fault_config = *config;
    }

    authorities
        .process_certificate(cert.into_cert_for_testing().into())
        .await
        .is_ok()
}

/// The intent of this is to test whether client side timeouts
/// have any impact on the server execution. Turns out because
/// we spawn a tokio task on the server, client timing out and
/// terminating the connection does not stop server from completing
/// execution on its side
#[sim_test(config = "constant_latency_ms(1)")]
async fn test_quorum_map_and_reduce_timeout() {
    let build_config = BuildConfig::new_for_testing();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/object_basics");
    let modules = sui_framework::build_move_package(&path, build_config)
        .unwrap()
        .get_modules()
        .into_iter()
        .cloned()
        .collect();
    let pkg = Object::new_package(modules, TransactionDigest::genesis()).unwrap();
    let (addr1, key1): (_, AccountKeyPair) = get_key_pair();
    let gas_object1 = Object::with_owner_for_testing(addr1);
    let genesis_objects = vec![pkg.clone(), gas_object1.clone()];
    let (mut authorities, _, genesis, _) = init_local_authorities(4, genesis_objects).await;
    let pkg = genesis.object(pkg.id()).unwrap();
    let gas_object1 = genesis.object(gas_object1.id()).unwrap();
    let gas_ref_1 = gas_object1.compute_object_reference();
    let tx = create_object_move_transaction(addr1, &key1, addr1, 100, pkg.id(), gas_ref_1);
    let certified_tx = authorities.process_transaction(tx.clone()).await;
    assert!(certified_tx.is_ok());
    let certificate = certified_tx.unwrap().into_cert_for_testing();
    // Send request with a very small timeout to trigger timeout error
    authorities.timeouts.pre_quorum_timeout = Duration::from_nanos(0);
    authorities.timeouts.post_quorum_timeout = Duration::from_nanos(0);
    let certified_effects = authorities
        .process_certificate(certificate.clone().into())
        .await;
    // Ensure it is an error
    assert!(certified_effects.is_err());
    assert!(matches!(
        certified_effects,
        Err(QuorumExecuteCertificateError { .. })
    ));
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    let tx_info = TransactionInfoRequest {
        transaction_digest: *tx.digest(),
    };
    for (_, client) in authorities.authority_clients.iter() {
        let resp = client
            .handle_transaction_info_request(tx_info.clone())
            .await;
        // Server should return a signed effect even though previous calls
        // failed due to timeout
        assert!(resp.is_ok());
        resp.unwrap().into_executed_for_testing();
    }
}

#[sim_test]
async fn test_map_reducer() {
    let (authorities, _, _, _) = init_local_authorities(4, vec![]).await;

    // Test: reducer errors get propagated up
    let res = authorities
        .quorum_map_then_reduce_with_timeout(
            0usize,
            |_name, _client| Box::pin(async move { Ok(()) }),
            |_accumulated_state, _authority_name, _authority_weight, _result| {
                Box::pin(async move {
                    Err(SuiError::TooManyIncorrectAuthorities {
                        errors: vec![],
                        action: "".to_string(),
                    })
                })
            },
            Duration::from_millis(1000),
        )
        .await;
    assert!(matches!(
        res,
        Err(SuiError::TooManyIncorrectAuthorities { .. })
    ));

    // Test: mapper errors do not get propagated up, reducer works
    let res = authorities
        .quorum_map_then_reduce_with_timeout(
            0usize,
            |_name, _client| {
                Box::pin(async move {
                    let res: Result<usize, SuiError> = Err(SuiError::TooManyIncorrectAuthorities {
                        errors: vec![],
                        action: "".to_string(),
                    });
                    res
                })
            },
            |mut accumulated_state, _authority_name, _authority_weight, result| {
                Box::pin(async move {
                    assert!(matches!(
                        result,
                        Err(SuiError::TooManyIncorrectAuthorities { .. })
                    ));
                    accumulated_state += 1;
                    Ok(ReduceOutput::Continue(accumulated_state))
                })
            },
            Duration::from_millis(1000),
        )
        .await;
    assert_eq!(Ok(4), res);

    // Test: early end
    let res = authorities
        .quorum_map_then_reduce_with_timeout(
            0usize,
            |_name, _client| Box::pin(async move { Ok(()) }),
            |mut accumulated_state, _authority_name, _authority_weight, _result| {
                Box::pin(async move {
                    if accumulated_state > 2 {
                        Ok(ReduceOutput::End(accumulated_state))
                    } else {
                        accumulated_state += 1;
                        Ok(ReduceOutput::Continue(accumulated_state))
                    }
                })
            },
            Duration::from_millis(1000),
        )
        .await;
    assert_eq!(Ok(3), res);

    // Test: Global timeout works
    let res = authorities
        .quorum_map_then_reduce_with_timeout(
            0usize,
            |_name, _client| {
                Box::pin(async move {
                    // 10 mins
                    tokio::time::sleep(Duration::from_secs(10 * 60)).await;
                    Ok(())
                })
            },
            |_accumulated_state, _authority_name, _authority_weight, _result| {
                Box::pin(async move {
                    Err(SuiError::TooManyIncorrectAuthorities {
                        errors: vec![],
                        action: "".to_string(),
                    })
                })
            },
            Duration::from_millis(10),
        )
        .await;
    assert_eq!(Ok(0), res);

    // Test: Local timeout works
    let bad_auth = *authorities.committee.sample();
    let res = authorities
        .quorum_map_then_reduce_with_timeout(
            HashSet::new(),
            |_name, _client| {
                Box::pin(async move {
                    // 10 mins
                    if _name == bad_auth {
                        tokio::time::sleep(Duration::from_secs(10 * 60)).await;
                    }
                    Ok(())
                })
            },
            |mut accumulated_state, authority_name, _authority_weight, _result| {
                Box::pin(async move {
                    accumulated_state.insert(authority_name);
                    if accumulated_state.len() <= 3 {
                        Ok(ReduceOutput::Continue(accumulated_state))
                    } else {
                        Ok(ReduceOutput::ContinueWithTimeout(
                            accumulated_state,
                            Duration::from_millis(10),
                        ))
                    }
                })
            },
            // large delay
            Duration::from_millis(10 * 60),
        )
        .await;
    assert_eq!(res.as_ref().unwrap().len(), 3);
    assert!(!res.as_ref().unwrap().contains(&bad_auth));
}

#[sim_test]
async fn test_process_transaction_fault_success() {
    // This test exercises the 4 different possible fauling case when one authority is faulty.
    // A transaction is sent to all authories, however one of them will error out either before or after processing the transaction.
    // A cert should still be created, and sent out to all authorities again. This time
    // a different authority errors out either before or after processing the cert.
    for i in 0..4 {
        let mut config_before_process_transaction = LocalAuthorityClientFaultConfig::default();
        if i % 2 == 0 {
            config_before_process_transaction.fail_before_handle_transaction = true;
        } else {
            config_before_process_transaction.fail_after_handle_transaction = true;
        }
        let mut config_before_process_certificate = LocalAuthorityClientFaultConfig::default();
        if i < 2 {
            config_before_process_certificate.fail_before_handle_confirmation = true;
        } else {
            config_before_process_certificate.fail_after_handle_confirmation = true;
        }
        assert!(
            execute_transaction_with_fault_configs(
                &[(0, config_before_process_transaction)],
                &[(1, config_before_process_certificate)],
            )
            .await
        );
    }
}

#[sim_test]
async fn test_process_transaction_fault_fail() {
    // This test exercises the cases when there are 2 authorities faulty,
    // and hence no quorum could be formed. This is tested on both the
    // process_transaction phase and process_certificate phase.
    let fail_before_process_transaction_config = LocalAuthorityClientFaultConfig {
        fail_before_handle_transaction: true,
        ..Default::default()
    };
    assert!(
        !execute_transaction_with_fault_configs(
            &[
                (0, fail_before_process_transaction_config),
                (1, fail_before_process_transaction_config),
            ],
            &[],
        )
        .await
    );

    let fail_before_process_certificate_config = LocalAuthorityClientFaultConfig {
        fail_before_handle_confirmation: true,
        ..Default::default()
    };
    assert!(
        !execute_transaction_with_fault_configs(
            &[],
            &[
                (0, fail_before_process_certificate_config),
                (1, fail_before_process_certificate_config),
            ],
        )
        .await
    );
}

#[tokio::test(start_paused = true)]
async fn test_quorum_once_with_timeout() {
    telemetry_subscribers::init_for_testing();

    let count = Arc::new(Mutex::new(0));
    let (authorities, _authorities_vec, clients) = get_authorities(count.clone(), 30);
    let agg = get_agg(authorities, clients, 0);

    let case = |agg: AuthorityAggregator<MockAuthorityApi>, authority_request_timeout: u64| async move {
        let log = Arc::new(Mutex::new(Vec::new()));
        let start = Instant::now();
        agg.quorum_once_with_timeout(
            None,
            None,
            |_name, client| {
                let digest = TransactionDigest::new([0u8; 32]);
                let log = log.clone();
                Box::pin(async move {
                    // log the start time of the request
                    log.lock().unwrap().push(Instant::now() - start);
                    let res = client
                        .handle_transaction_info_request(TransactionInfoRequest {
                            transaction_digest: digest,
                        })
                        .await;
                    match res {
                        Ok(_) => Ok(()),
                        // Treat transaction not found OK just to test timeout functionality.
                        Err(SuiError::TransactionNotFound { .. }) => Ok(()),
                        Err(err) => Err(err),
                    }
                })
            },
            Duration::from_millis(authority_request_timeout),
            Some(Duration::from_millis(30 * 50)),
            "test".to_string(),
        )
        .await
        .unwrap();
        Arc::try_unwrap(log).unwrap().into_inner().unwrap()
    };

    // New requests are started every 50ms even though each request hangs for 1000ms.
    // The 15th request succeeds, and we exit before processing the remaining authorities.
    assert_eq!(
        case(agg.clone(), 1000).await,
        (0..15)
            .map(|d| Duration::from_millis(d * 50))
            .collect::<Vec<Duration>>()
    );

    *count.lock().unwrap() = 0;
    // Here individual requests time out relatively quickly (100ms), but we continue increasing
    // the parallelism every 50ms
    assert_eq!(
        case(agg.clone(), 100).await,
        [0, 50, 100, 100, 150, 150, 200, 200, 200, 250, 250, 250, 300, 300, 300]
            .iter()
            .map(|d| Duration::from_millis(*d))
            .collect::<Vec<Duration>>()
    );
}

#[allow(clippy::type_complexity)]
fn get_authorities(
    count: Arc<Mutex<u32>>,
    committee_size: u64,
) -> (
    BTreeMap<AuthorityName, StakeUnit>,
    Vec<(AuthorityName, StakeUnit)>,
    BTreeMap<AuthorityName, MockAuthorityApi>,
) {
    let new_client = |delay: u64| {
        let delay = Duration::from_millis(delay);
        let count = count.clone();
        MockAuthorityApi::new(delay, count)
    };

    let mut authorities = BTreeMap::new();
    let mut authorities_vec = Vec::new();
    let mut clients = BTreeMap::new();
    for _ in 0..committee_size {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let name: AuthorityName = sec.public().into();
        authorities.insert(name, 1);
        authorities_vec.push((name, 1));
        clients.insert(name, new_client(1000));
    }
    (authorities, authorities_vec, clients)
}

fn get_agg<A>(
    authorities: BTreeMap<AuthorityName, StakeUnit>,
    clients: BTreeMap<AuthorityName, A>,
    epoch: EpochId,
) -> AuthorityAggregator<A> {
    let committee = Committee::new(epoch, ProtocolVersion::MIN, authorities).unwrap();
    let committee_store = Arc::new(CommitteeStore::new_for_testing(&committee));

    AuthorityAggregator::new_with_timeouts(
        committee,
        committee_store,
        clients,
        &Registry::new(),
        TimeoutConfig {
            serial_authority_request_interval: Duration::from_millis(50),
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn test_get_committee_with_net_addresses() {
    telemetry_subscribers::init_for_testing();
    let count = Arc::new(Mutex::new(0));
    let new_client = |delay: u64| {
        let delay = Duration::from_millis(delay);
        let count = count.clone();
        MockAuthorityApi::new(delay, count)
    };

    let (val0_pk, val0_addr) = get_authority_pub_key_bytes_and_address();
    let (val1_pk, val1_addr) = get_authority_pub_key_bytes_and_address();
    let (val2_pk, val2_addr) = get_authority_pub_key_bytes_and_address();
    let (val3_pk, val3_addr) = get_authority_pub_key_bytes_and_address();

    let mut clients = BTreeMap::from([
        (val0_pk, new_client(1000)),
        (val1_pk, new_client(1000)),
        (val2_pk, new_client(1000)),
        (val3_pk, new_client(1000)),
    ]);
    let authorities = BTreeMap::from([(val0_pk, 1), (val1_pk, 1), (val2_pk, 1), (val3_pk, 1)]);

    let validators = vec![
        test_validator(val0_pk, Multiaddr::empty().to_vec(), 1, 0),
        test_validator(val1_pk, Multiaddr::empty().to_vec(), 1, 0),
        test_validator(val2_pk, Multiaddr::empty().to_vec(), 1, 0),
        test_validator(val3_pk, Multiaddr::empty().to_vec(), 1, 0),
    ];
    let system_state = test_sui_system_state(1, validators);
    let good_result = make_response_from_sui_system_state(system_state.clone());

    for client in clients.values_mut() {
        client.set_handle_object_info_request(good_result.clone());
    }
    let clients = clients;
    let agg = get_agg(authorities.clone(), clients.clone(), 0);
    let res = agg.get_committee_with_net_addresses(1).await;

    macro_rules! verify_good_result {
        ($res: expr, $epoch: expr) => {{
            let res = $res;
            match res {
                Ok(info) => {
                    assert_eq!(info.committee.epoch, $epoch);
                    assert_eq!(
                        info.committee
                            .voting_rights
                            .into_iter()
                            .collect::<BTreeMap<_, _>>(),
                        BTreeMap::from([(val0_pk, 1), (val1_pk, 1), (val2_pk, 1), (val3_pk, 1),]),
                    );
                    assert_eq!(
                        info.net_addresses,
                        BTreeMap::from([
                            (val0_pk, val0_addr.clone()),
                            (val1_pk, val1_addr.clone()),
                            (val2_pk, val2_addr.clone()),
                            (val3_pk, val3_addr.clone()),
                        ])
                    );
                }
                Err(err) => panic!("expect Ok result but got {err}"),
            };
        }};
    }

    macro_rules! verify_bad_result {
        ($res: expr, $epoch: expr) => {{
            let res = $res;
            match res {
                Ok(info) => panic!(
                    "expect SuiError::FailedToGetAgreedCommitteeFromMajority but got {:?}",
                    info
                ),
                Err(SuiError::FailedToGetAgreedCommitteeFromMajority { minimal_epoch }) => {
                    assert_eq!(minimal_epoch, $epoch);
                }
                Err(err) => panic!(
                    "expect SuiError::FailedToGetAgreedCommitteeFromMajority but got {:?}",
                    err
                ),
            };
        }};
    }
    verify_good_result!(res, 1);

    // 1 out of 4 gives bad result, we are good
    let mut clone_clients = clients.clone();
    let bad_result: SuiResult<ObjectInfoResponse> = Err(SuiError::GenericAuthorityError {
        error: "foo".into(),
    });

    clone_clients
        .get_mut(&val0_pk)
        .unwrap()
        .set_handle_object_info_request(bad_result.clone());

    let agg = get_agg(authorities.clone(), clone_clients.clone(), 0);
    let res = agg.get_committee_with_net_addresses(1).await;

    verify_good_result!(res, 1);

    // 2 out of 4 give bad result, get error
    clone_clients
        .get_mut(&val1_pk)
        .unwrap()
        .set_handle_object_info_request(bad_result.clone());
    let agg = get_agg(authorities.clone(), clone_clients.clone(), 0);
    let res = agg.get_committee_with_net_addresses(1).await;
    verify_bad_result!(res, 1);

    // val0 and val1 gives a slightly different system state but
    // CommitteeWithNetAddresses is the same, we are good.
    let mut system_state_clone = system_state.clone();
    // In practice we wouldn't expect validator_stake differs in the same epoch.
    // Here we update it for simplicity.
    system_state_clone.validators.validator_stake += 1;
    let different_result = make_response_from_sui_system_state(system_state_clone);

    let mut clone_clients = clients.clone();
    clone_clients
        .get_mut(&val0_pk)
        .unwrap()
        .set_handle_object_info_request(different_result.clone());
    clone_clients
        .get_mut(&val1_pk)
        .unwrap()
        .set_handle_object_info_request(different_result.clone());

    let agg = get_agg(authorities.clone(), clone_clients.clone(), 0);
    let res = agg.get_committee_with_net_addresses(1).await;
    verify_good_result!(res, 1);

    // (val0, val1) disagree with (val2, val3) on network address, get error
    let validators = vec![
        test_validator(
            val0_pk,
            "/ip4/127.0.0.1".parse::<Multiaddr>().unwrap().to_vec(),
            1,
            0,
        ),
        test_validator(val1_pk, Multiaddr::empty().to_vec(), 1, 0),
        test_validator(val2_pk, Multiaddr::empty().to_vec(), 1, 0),
        test_validator(val3_pk, Multiaddr::empty().to_vec(), 1, 0),
    ];
    let system_state_with_different_net_addr = test_sui_system_state(1, validators);
    let different_result =
        make_response_from_sui_system_state(system_state_with_different_net_addr);

    clone_clients
        .get_mut(&val0_pk)
        .unwrap()
        .set_handle_object_info_request(different_result.clone());
    clone_clients
        .get_mut(&val1_pk)
        .unwrap()
        .set_handle_object_info_request(different_result.clone());

    let agg = get_agg(authorities.clone(), clone_clients, 0);
    let res = agg.get_committee_with_net_addresses(1).await;
    verify_bad_result!(res, 1);

    // val0, val1 and val2 are still in epoch0
    let mut system_state_clone = system_state.clone();
    system_state_clone.epoch = 0;
    let epoch_0_result = make_response_from_sui_system_state(system_state_clone);

    let mut clone_clients = clients.clone();
    clone_clients
        .get_mut(&val0_pk)
        .unwrap()
        .set_handle_object_info_request(epoch_0_result.clone());
    clone_clients
        .get_mut(&val1_pk)
        .unwrap()
        .set_handle_object_info_request(epoch_0_result.clone());
    clone_clients
        .get_mut(&val2_pk)
        .unwrap()
        .set_handle_object_info_request(epoch_0_result.clone());
    let agg = get_agg(authorities.clone(), clone_clients, 0);
    let res = agg.get_committee_with_net_addresses(1).await;
    // Get error when asking with minimal epoch = 1
    verify_bad_result!(res, 1);
    // Get good results when asking with minimal epoch = 0
    let res = agg.get_committee_with_net_addresses(0).await;
    verify_good_result!(res, 0);
}

#[tokio::test]
async fn test_get_committee_info() {
    telemetry_subscribers::init_for_testing();

    let count = Arc::new(Mutex::new(0));
    // 4 out of 4 give good result
    let (authorities, authorities_vec, mut clients) = get_authorities(count.clone(), 4);
    let good_result = Ok(CommitteeInfoResponse {
        epoch: 0,
        protocol_version: ProtocolVersion::MIN,
        committee_info: authorities_vec.clone(),
    });
    for client in clients.values_mut() {
        client.set_handle_committee_info_request_result(good_result.clone());
    }
    let clients = clients;
    let clone_clients = clients.clone();
    let agg = get_agg(authorities.clone(), clone_clients, 0);
    let res = agg.get_committee_info(Some(0)).await;
    match res {
        Ok(info) => {
            assert_eq!(info.epoch, 0);
            assert_eq!(info.committee_info, authorities_vec);
        }
        Err(err) => panic!("expect Ok result but got {err}"),
    };

    // 1 out 4 gives error
    let mut clone_clients = clients.clone();
    let bad_result = Err(SuiError::GenericAuthorityError {
        error: "foo".into(),
    });

    clone_clients
        .values_mut()
        .next()
        .unwrap()
        .set_handle_committee_info_request_result(bad_result.clone());
    let agg = get_agg(authorities.clone(), clone_clients, 0);
    let res = agg.get_committee_info(Some(0)).await;
    match res {
        Ok(info) => {
            assert_eq!(info.epoch, 0);
            assert_eq!(info.committee_info, authorities_vec);
        }
        Err(_) => panic!("expect Ok result!"),
    };

    // 2 out 4 gives error
    let mut clone_clients = clients.clone();
    let mut i = 0;
    for client in clone_clients.values_mut() {
        client.set_handle_committee_info_request_result(bad_result.clone());
        i += 1;
        if i >= 2 {
            break;
        }
    }
    let agg = get_agg(authorities.clone(), clone_clients, 0);
    let res = agg.get_committee_info(Some(0)).await;
    match res {
        Err(SuiError::TooManyIncorrectAuthorities { .. }) => (),
        other => panic!(
            "expect to get SuiError::TooManyIncorrectAuthorities but got {:?}",
            other
        ),
    };
}

fn sign_tx(
    tx: VerifiedTransaction,
    epoch: EpochId,
    authority: AuthorityName,
    secret: &dyn signature::Signer<AuthoritySignature>,
) -> SignedTransaction {
    SignedTransaction::new(epoch, tx.into_inner().into_data(), secret, authority)
}

fn sign_tx_effects(
    effects: TransactionEffects,
    epoch: EpochId,
    authority: AuthorityName,
    secret: &dyn signature::Signer<AuthoritySignature>,
) -> SignedTransactionEffects {
    SignedTransactionEffects::new(epoch, effects, secret, authority)
}

#[tokio::test]
async fn test_handle_transaction_response() {
    telemetry_subscribers::init_for_testing();

    let mut authorities = BTreeMap::new();
    let mut clients = BTreeMap::new();
    let mut authority_keys = Vec::new();
    for _ in 0..4 {
        let (_, sec): (_, AuthorityKeyPair) = get_key_pair();
        let name: AuthorityName = sec.public().into();
        authorities.insert(name, 1);
        authority_keys.push((name, sec));
        clients.insert(name, HandleTransactionTestAuthorityClient::new());
    }

    let (sender, sender_kp): (_, AccountKeyPair) = get_key_pair();
    let tx = make_transfer_sui_transaction(
        random_object_ref(),
        SuiAddress::default(),
        None,
        sender,
        &sender_kp,
        None,
    );
    // Case 0
    // Validators give invalid response because of the initial value set for their responses.
    let agg = get_agg(authorities.clone(), clients.clone(), 0);

    assert_resp_err(&agg, tx.clone(), |e| matches!(e, SuiError::Unknown(..))).await;

    // Case 1
    // All Validators gives signed-tx
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx, 0);
    // Validators now gives valid signed tx and we get TxCert
    let mut agg = get_agg(authorities.clone(), clients.clone(), 0);
    let cert_epoch_0 = agg
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();

    // Case 2
    // Validators return signed-tx with epoch 0, client expects 1
    // Update client to epoch 1
    let committee_1 = Committee::new(1, ProtocolVersion::MIN, authorities.clone()).unwrap();
    agg.committee_store
        .insert_new_committee(&committee_1)
        .unwrap();
    agg.committee = committee_1;

    assert_resp_err(&agg, tx.clone(),
        |e| matches!(e, SuiError::WrongEpoch { expected_epoch, actual_epoch } if *expected_epoch == 1 && *actual_epoch == 0)
    ).await;

    // Case 3
    // Val-0 returns tx-cert
    let effects = TransactionEffects {
        transaction_digest: *cert_epoch_0.digest(),
        ..Default::default()
    };
    let (name_0, key_0) = &authority_keys[0];
    let resp = TransactionInfoResponse::Executed(
        cert_epoch_0.clone().into_inner(),
        sign_tx_effects(effects, 0, *name_0, key_0),
    );
    clients
        .get_mut(&authority_keys[0].0)
        .unwrap()
        .set_tx_info_response(resp);

    // Val-3 returns invalid response
    // (Val-1 and Val-2 returns signed-tx)
    for (name, _) in authority_keys.iter().skip(3) {
        clients.get_mut(name).unwrap().reset_tx_info_response();
    }
    let agg = get_agg(authorities.clone(), clients.clone(), 0);
    // We have a valid cert because val-0 has it. Note we can't form a cert based on what val-1 and val-2 give
    agg.process_transaction(tx.clone()).await.unwrap();

    // Case 4
    // Validators return signed-tx with epoch 1, client expects 0
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx, 1);

    let mut agg = get_agg(authorities.clone(), clients.clone(), 0);

    assert_resp_err(
        &agg,
        tx.clone(),
        |e| matches!(e, SuiError::MissingCommitteeAtEpoch(e) if *e == 1),
    )
    .await;

    let committee_1 = Committee::new(1, ProtocolVersion::MIN, authorities.clone()).unwrap();
    agg.committee_store
        .insert_new_committee(&committee_1)
        .unwrap();
    agg.committee = committee_1.clone();
    let cert_epoch_1 = agg
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();

    // Case 5
    // Validators return tx-cert with epoch 0, client expects 1
    let effects = TransactionEffects {
        transaction_digest: *cert_epoch_0.digest(),
        ..Default::default()
    };
    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter(),
        cert_epoch_0.inner(),
        effects.clone(),
        0,
    );

    // Update client to epoch 1
    let mut agg = get_agg(authorities.clone(), clients.clone(), 0);
    agg.committee_store
        .insert_new_committee(&committee_1)
        .unwrap();
    agg.committee = committee_1.clone();
    // Err because either cert or signed effects is in epoch 0
    assert_resp_err(&agg, tx.clone(),
        |e| matches!(e, SuiError::WrongEpoch { expected_epoch, actual_epoch } if *expected_epoch == 1 && *actual_epoch == 0)
    ).await;

    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter(),
        cert_epoch_0.inner(),
        effects,
        1,
    );
    let mut agg = get_agg(authorities.clone(), clients.clone(), 0);
    agg.committee_store
        .insert_new_committee(&committee_1)
        .unwrap();
    agg.committee = committee_1.clone();
    // We have 2f+1 signed effects on epoch 1, so we are good.
    agg.process_transaction(tx.clone()).await.unwrap();

    // Case 6
    // Validators 2 and 3 returns tx-cert with epoch 1, but different signed effects from 0 and 1
    let effects = TransactionEffects {
        transaction_digest: *cert_epoch_0.digest(),
        status: ExecutionStatus::Failure {
            error: ExecutionFailureStatus::InsufficientGas,
        },
        ..Default::default()
    };
    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter().skip(2),
        cert_epoch_0.inner(),
        effects,
        1,
    );

    let mut agg = get_agg(authorities.clone(), clients.clone(), 0);
    agg.committee_store
        .insert_new_committee(&committee_1)
        .unwrap();
    agg.committee = committee_1.clone();

    assert_resp_err(&agg, tx.clone(), |e| {
        matches!(
            e,
            SuiError::QuorumFailedToGetEffectsQuorumWhenProcessingTransaction { .. }
        )
    })
    .await;

    // Case 7
    // Validators return tx-cert with epoch 1, client expects 0
    let effects = TransactionEffects {
        transaction_digest: *cert_epoch_1.digest(),
        ..Default::default()
    };
    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter(),
        cert_epoch_1.inner(),
        effects,
        1,
    );
    let agg = get_agg(authorities.clone(), clients.clone(), 0);

    assert_resp_err(
        &agg,
        tx.clone(),
        |e| matches!(e, SuiError::MissingCommitteeAtEpoch(e) if *e == 1),
    )
    .await;

    // Update committee store, now SafeClinet will pass
    agg.committee_store
        .insert_new_committee(&committee_1)
        .unwrap();

    assert_resp_err(
        &agg,
        tx.clone(),
        |e| matches!(e, SuiError::WrongEpoch { expected_epoch, actual_epoch } if *expected_epoch == 0 && *actual_epoch == 1)
    )
    .await;
}

async fn assert_resp_err<F>(
    agg: &AuthorityAggregator<HandleTransactionTestAuthorityClient>,
    tx: VerifiedTransaction,
    checker: F,
) where
    F: Fn(&SuiError) -> bool,
{
    match agg.process_transaction(tx).await {
        Err(QuorumSignTransactionError {
            total_stake,
            good_stake,
            errors,
            conflicting_tx_digests,
        }) => {
            assert_eq!(total_stake, 4);
            assert_eq!(good_stake, 0);
            assert!(conflicting_tx_digests.is_empty());
            assert!(errors.iter().map(|e| &e.0).all(checker));
        }
        other => {
            panic!(
                "Expect QuorumFailedToProcessTransaction but got {:?}",
                other
            );
        }
    }
}

fn set_tx_info_response_with_cert_and_effects<'a>(
    clients: &mut BTreeMap<AuthorityName, HandleTransactionTestAuthorityClient>,
    authority_keys: impl Iterator<Item = &'a (AuthorityName, AuthorityKeyPair)>,
    cert: &CertifiedTransaction,
    effects: TransactionEffects,
    epoch: EpochId,
) {
    for (name, key) in authority_keys {
        let resp = TransactionInfoResponse::Executed(
            cert.clone(),
            SignedTransactionEffects::new(epoch, effects.clone(), key, *name),
        );
        clients.get_mut(name).unwrap().set_tx_info_response(resp);
    }
}

fn set_tx_info_response_with_signed_tx(
    clients: &mut BTreeMap<AuthorityName, HandleTransactionTestAuthorityClient>,
    authority_keys: &Vec<(AuthorityName, AuthorityKeyPair)>,
    tx: &VerifiedTransaction,
    epoch: EpochId,
) {
    for (name, secret) in authority_keys {
        let signed_tx = sign_tx(tx.clone(), epoch, *name, secret);

        let resp = TransactionInfoResponse::Signed(signed_tx);
        clients.get_mut(name).unwrap().set_tx_info_response(resp);
    }
}

pub fn make_response_from_sui_system_state(
    system_state: SuiSystemState,
) -> SuiResult<ObjectInfoResponse> {
    let move_content = to_bytes(&system_state).unwrap();
    let tx_cert = make_random_certified_transaction();
    let move_object = unsafe {
        MoveObject::new_from_execution(
            SuiSystemState::type_(),
            false,
            SequenceNumber::from_u64(1),
            move_content,
        )
        .unwrap()
    };
    let initial_shared_version = move_object.version();
    let object = Object::new_move(
        move_object,
        Owner::Shared {
            initial_shared_version,
        },
        *tx_cert.digest(),
    );
    let obj_digest = object.compute_object_reference();
    Ok(ObjectInfoResponse {
        parent_certificate: Some(tx_cert.into()),
        requested_object_reference: Some(obj_digest),
        object_and_lock: Some(ObjectResponse {
            object,
            lock: None,
            layout: None,
        }),
    })
}

pub fn get_authority_pub_key_bytes_and_address() -> (AuthorityPublicKeyBytes, Vec<u8>) {
    let (_val0_addr, val0_kp) = get_authority_key_pair();
    (
        AuthorityPublicKeyBytes::from(val0_kp.public()),
        Multiaddr::empty().to_vec(),
    )
}
