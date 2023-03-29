// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::test_utils::make_transfer_object_transaction;
use crate::test_utils::make_transfer_sui_transaction;
use move_core_types::{account_address::AccountAddress, ident_str};
use rand::rngs::StdRng;
use rand::SeedableRng;
use shared_crypto::intent::{Intent, IntentScope};
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use sui_move_build::BuildConfig;
use sui_types::crypto::get_key_pair_from_rng;
use sui_types::crypto::{get_key_pair, AccountKeyPair, AuthorityKeyPair};
use sui_types::crypto::{AuthoritySignature, Signer};
use sui_types::crypto::{KeypairTraits, Signature};
use sui_types::utils::create_fake_transaction;

use sui_macros::sim_test;
use sui_types::messages::*;
use sui_types::object::Object;

use super::*;
use crate::authority_client::AuthorityAPI;
use crate::test_authority_clients::{
    HandleTransactionTestAuthorityClient, LocalAuthorityClient, LocalAuthorityClientFaultConfig,
    MockAuthorityApi,
};
use crate::test_utils::init_local_authorities;
use sui_framework::BuiltInFramework;
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

pub fn create_object_move_transaction(
    src: SuiAddress,
    secret: &dyn Signer<Signature>,
    dest: SuiAddress,
    value: u64,
    package_id: ObjectID,
    gas_object_ref: ObjectRef,
    gas_price: u64,
) -> VerifiedTransaction {
    // When creating an object_basics object, we provide the value (u64) and address which will own the object
    let arguments = vec![
        CallArg::Pure(value.to_le_bytes().to_vec()),
        CallArg::Pure(bcs::to_bytes(&AccountAddress::from(dest)).unwrap()),
    ];

    to_sender_signed_transaction(
        TransactionData::new_move_call(
            src,
            package_id,
            ident_str!("object_basics").to_owned(),
            ident_str!("create").to_owned(),
            Vec::new(),
            gas_object_ref,
            arguments,
            TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * gas_price,
            gas_price,
        )
        .unwrap(),
        secret,
    )
}

pub fn delete_object_move_transaction(
    src: SuiAddress,
    secret: &dyn Signer<Signature>,
    object_ref: ObjectRef,
    framework_obj_id: ObjectID,
    gas_object_ref: ObjectRef,
    gas_price: u64,
) -> VerifiedTransaction {
    to_sender_signed_transaction(
        TransactionData::new_move_call(
            src,
            framework_obj_id,
            ident_str!("object_basics").to_owned(),
            ident_str!("delete").to_owned(),
            Vec::new(),
            gas_object_ref,
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref))],
            TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * gas_price,
            gas_price,
        )
        .unwrap(),
        secret,
    )
}

pub fn set_object_move_transaction(
    src: SuiAddress,
    secret: &dyn Signer<Signature>,
    object_ref: ObjectRef,
    value: u64,
    framework_obj_id: ObjectID,
    gas_object_ref: ObjectRef,
    gas_price: u64,
) -> VerifiedTransaction {
    let args = vec![
        CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)),
        CallArg::Pure(bcs::to_bytes(&value).unwrap()),
    ];

    to_sender_signed_transaction(
        TransactionData::new_move_call(
            src,
            framework_obj_id,
            ident_str!("object_basics").to_owned(),
            ident_str!("set_value").to_owned(),
            Vec::new(),
            gas_object_ref,
            args,
            TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * gas_price,
            gas_price,
        )
        .unwrap(),
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
    let mut tx_data: Option<SenderSignedData> = None;
    for authority in authorities {
        let response = authority
            .handle_transaction_info_request(TransactionInfoRequest {
                transaction_digest: *transaction_digest,
            })
            .await;
        match response {
            Ok(PlainTransactionInfoResponse::Signed(signed)) => {
                let (data, sig) = signed.into_data_and_sig();
                votes.push(sig);
                if let Some(inner_transaction) = tx_data {
                    assert_eq!(
                        inner_transaction.intent_message().value,
                        data.intent_message().value
                    );
                }
                tx_data = Some(data);
            }
            Ok(PlainTransactionInfoResponse::ExecutedWithCert(cert, _, _)) => {
                return cert.into_inner();
            }
            _ => {}
        }
    }

    CertifiedTransaction::new(tx_data.unwrap(), &votes, committee).unwrap()
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
        .into_data()
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
    if let Ok(VerifiedObjectInfoResponse { object }) = authority
        .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
            object_id, None,
        ))
        .await
    {
        return object.compute_object_reference();
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
    let rgp = genesis.reference_gas_price();
    let tx = make_transfer_object_transaction(
        gas_object1.compute_object_reference(),
        gas_object2.compute_object_reference(),
        addr1,
        &key1,
        addr2,
        rgp,
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

fn effects_with_tx(digest: TransactionDigest) -> TransactionEffects {
    let mut effects = TransactionEffects::default();
    *effects.transaction_digest_mut_for_testing() = digest;
    effects
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
    path.extend(["src", "unit_tests", "data", "object_basics"]);
    let modules: Vec<_> = build_config
        .build(path)
        .unwrap()
        .get_modules()
        .cloned()
        .collect();
    let pkg = Object::new_package_for_testing(
        &modules,
        TransactionDigest::genesis(),
        BuiltInFramework::genesis_move_packages(),
    )
    .unwrap();
    let (addr1, key1): (_, AccountKeyPair) = get_key_pair();
    let gas_object1 = Object::with_owner_for_testing(addr1);
    let genesis_objects = vec![pkg.clone(), gas_object1.clone()];
    let (mut authorities, _, genesis, _) = init_local_authorities(4, genesis_objects).await;
    let rgp = genesis.reference_gas_price();
    let pkg = genesis.object(pkg.id()).unwrap();
    let gas_object1 = genesis.object(gas_object1.id()).unwrap();
    let gas_ref_1 = gas_object1.compute_object_reference();
    let tx = create_object_move_transaction(addr1, &key1, addr1, 100, pkg.id(), gas_ref_1, rgp);
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
        Err(AggregatorProcessCertificateError::RetryableExecuteCertificate { .. })
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
        assert!(resp.unwrap().is_executed());
    }
}

#[sim_test]
async fn test_map_reducer() {
    let (authorities, _, _, _) = init_local_authorities(4, vec![]).await;

    // Test: mapper errors do not get propagated up, reducer works
    let res: Result<(), usize> = authorities
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
                    ReduceOutput::Continue(accumulated_state)
                })
            },
            Duration::from_millis(1000),
        )
        .await;
    assert_eq!(Err(4), res);

    // Test: early end
    let res = authorities
        .quorum_map_then_reduce_with_timeout(
            0usize,
            |_name, _client| Box::pin(async move { Ok(()) }),
            |mut accumulated_state, _authority_name, _authority_weight, _result| {
                Box::pin(async move {
                    if accumulated_state > 2 {
                        ReduceOutput::Success(accumulated_state)
                    } else {
                        accumulated_state += 1;
                        ReduceOutput::Continue(accumulated_state)
                    }
                })
            },
            Duration::from_millis(1000),
        )
        .await;
    assert_eq!(Ok(3), res);

    // Test: Global timeout works
    let res: Result<(), _> = authorities
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
                Box::pin(async move { ReduceOutput::Continue(0) })
            },
            Duration::from_millis(10),
        )
        .await;
    assert_eq!(Err(0), res);

    // Test: Local timeout works
    let bad_auth = *authorities.committee.sample();
    let res: Result<(), _> = authorities
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
                        ReduceOutput::Continue(accumulated_state)
                    } else {
                        ReduceOutput::ContinueWithTimeout(
                            accumulated_state,
                            Duration::from_millis(10),
                        )
                    }
                })
            },
            // large delay
            Duration::from_millis(10 * 60),
        )
        .await;
    assert_eq!(res.as_ref().unwrap_err().len(), 3);
    assert!(!res.as_ref().unwrap_err().contains(&bad_auth));
}

#[sim_test]
async fn test_process_transaction_fault_success() {
    // This test exercises the 4 different possible failing case when one authority is faulty.
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
    let agg = get_genesis_agg(authorities, clients);

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

fn get_genesis_agg<A>(
    authorities: BTreeMap<AuthorityName, StakeUnit>,
    clients: BTreeMap<AuthorityName, A>,
) -> AuthorityAggregator<A> {
    let committee = Committee::new_for_testing_with_normalized_voting_power(0, authorities);
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

fn get_agg_at_epoch<A>(
    authorities: BTreeMap<AuthorityName, StakeUnit>,
    clients: BTreeMap<AuthorityName, A>,
    epoch: EpochId,
) -> AuthorityAggregator<A>
where
    A: Clone,
{
    let mut agg = get_genesis_agg(authorities.clone(), clients);
    let committee = Committee::new_for_testing_with_normalized_voting_power(epoch, authorities);
    agg.committee_store
        .insert_new_committee(&committee)
        .unwrap();
    agg.committee = committee;
    agg
}

fn sign_tx(
    tx: VerifiedTransaction,
    epoch: EpochId,
    authority: AuthorityName,
    secret: &dyn Signer<AuthoritySignature>,
) -> SignedTransaction {
    SignedTransaction::new(epoch, tx.into_inner().into_data(), secret, authority)
}

fn sign_tx_effects(
    effects: TransactionEffects,
    epoch: EpochId,
    authority: AuthorityName,
    secret: &dyn Signer<AuthoritySignature>,
) -> SignedTransactionEffects {
    SignedTransactionEffects::new(epoch, effects, secret, authority)
}

#[tokio::test]
#[should_panic]
async fn test_handle_transaction_panic() {
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
    let gas_object = random_object_ref();
    let tx = make_transfer_sui_transaction(
        gas_object,
        SuiAddress::default(),
        None,
        sender,
        &sender_kp,
        666, // this is a dummy value which does not matter
    );

    // Non-quorum of effects without a retryable majority indicating a safety violation
    // or a fork

    // All Validators gives signed-tx
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx, 0);

    // Validators now gives valid signed tx and we get TxCert
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    let cert_epoch_0 = agg
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();

    // Validators 2 and 3 return successful effects
    let effects = effects_with_tx(*cert_epoch_0.digest());
    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter(),
        Some(cert_epoch_0.inner()),
        effects,
        1,
    );

    // Validator 0 and 1 return failed effects
    let effects = TransactionEffectsV1 {
        transaction_digest: *cert_epoch_0.digest(),
        status: ExecutionStatus::Failure {
            error: ExecutionFailureStatus::InsufficientGas,
            command: None,
        },
        ..Default::default()
    };
    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter().skip(2),
        Some(cert_epoch_0.inner()),
        TransactionEffects::V1(effects),
        1,
    );
    let agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 1);

    // We have forked, should panic
    let _ = agg.process_transaction(tx.clone()).await;
}

#[tokio::test]
async fn test_handle_transaction_response() {
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
    let gas_object = random_object_ref();
    let tx = make_transfer_sui_transaction(
        gas_object,
        SuiAddress::default(),
        None,
        sender,
        &sender_kp,
        666, // this is a dummy value which does not matter
    );
    let tx2 = make_transfer_sui_transaction(
        gas_object,
        SuiAddress::default(),
        Some(1),
        sender,
        &sender_kp,
        666, // this is a dummy value which does not matter
    );
    let package_not_found_error = SuiError::UserInputError {
        error: UserInputError::DependentPackageNotFound {
            package_id: gas_object.0,
        },
    };
    let object_not_found_error = SuiError::UserInputError {
        error: UserInputError::ObjectNotFound {
            object_id: gas_object.0,
            version: Some(gas_object.1),
        },
    };

    println!("Case 0 - Non-retryable Transaction (Unknown Error)");
    // Validators give invalid response because of the initial value set for their responses.
    let agg = get_genesis_agg(authorities.clone(), clients.clone());

    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::FatalTransaction { .. }
            )
        },
        |e| matches!(e, SuiError::Unknown(..)),
    )
    .await;

    println!("Case 1 - Successful Signed Transaction");
    // All Validators gives signed-tx
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx, 0);

    // Validators now gives valid signed tx and we get TxCert
    let mut agg = get_genesis_agg(authorities.clone(), clients.clone());
    let cert_epoch_0 = agg
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();

    println!("Case 2 - Retryable Transaction (WrongEpoch Error)");
    // Validators return signed-tx with epoch 0, client expects 1
    // Update client to epoch 1
    let committee_1 =
        Committee::new_for_testing_with_normalized_voting_power(1, authorities.clone());
    agg.committee_store
        .insert_new_committee(&committee_1)
        .unwrap();
    agg.committee = committee_1;

    assert_resp_err(&agg, tx.clone(), |e| matches!(e, AggregatorProcessTransactionError::RetryableTransaction { .. }),
        |e| matches!(e, SuiError::WrongEpoch { expected_epoch, actual_epoch } if *expected_epoch == 1 && *actual_epoch == 0)
    ).await;

    println!("Case 3 - Successful Cert Transaction");
    // Val-0 returns tx-cert
    let effects = effects_with_tx(*cert_epoch_0.digest());
    let (name_0, key_0) = &authority_keys[0];
    let resp = HandleTransactionResponse {
        status: TransactionStatus::Executed(
            Some(cert_epoch_0.auth_sig().clone()),
            sign_tx_effects(effects, 0, *name_0, key_0),
            TransactionEvents { data: vec![] },
        ),
    };
    clients
        .get_mut(&authority_keys[0].0)
        .unwrap()
        .set_tx_info_response(resp);

    // Val-3 returns invalid response
    // (Val-1 and Val-2 returns signed-tx)
    for (name, _) in authority_keys.iter().skip(3) {
        clients.get_mut(name).unwrap().reset_tx_info_response();
    }
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    // We have a valid cert because val-0 has it. Note we can't form a cert based on what val-1 and val-2 give
    agg.process_transaction(tx.clone()).await.unwrap();

    println!("Case 4 - Retryable Transaction (MissingCommitteeAtEpoch Error)");
    // Validators return signed-tx with epoch 1, client expects 0
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx, 1);

    let mut agg = get_genesis_agg(authorities.clone(), clients.clone());

    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableTransaction { .. }
            )
        },
        |e| matches!(e, SuiError::MissingCommitteeAtEpoch(e) if *e == 1),
    )
    .await;

    let committee_1 =
        Committee::new_for_testing_with_normalized_voting_power(1, authorities.clone());
    agg.committee_store
        .insert_new_committee(&committee_1)
        .unwrap();
    agg.committee = committee_1.clone();

    let cert_epoch_1 = agg
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();

    println!("Case 5 - Retryable Transaction (WrongEpoch Error)");
    // Validators return tx-cert with epoch 0, client expects 1
    let effects = effects_with_tx(*cert_epoch_0.digest());
    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter(),
        Some(cert_epoch_0.inner()),
        effects.clone(),
        0,
    );

    // Update client to epoch 1
    let agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 1);

    // Err because either cert or signed effects is in epoch 0
    assert_resp_err(&agg, tx.clone(), |e| matches!(e, AggregatorProcessTransactionError::RetryableTransaction { .. }),
        |e| matches!(e, SuiError::WrongEpoch { expected_epoch, actual_epoch } if *expected_epoch == 1 && *actual_epoch == 0)
    ).await;

    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter(),
        Some(cert_epoch_0.inner()),
        effects,
        1,
    );
    let agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 1);
    // We have 2f+1 signed effects on epoch 1, so we are good.
    agg.process_transaction(tx.clone()).await.unwrap();

    println!("Case 6 - Retryable Transaction (most staked effects stake + retryable stake >= 2f+1 with QuorumFailedToGetEffectsQuorumWhenProcessingTransaction Error)");
    // Val 0, 1 & 2 returns retryable error
    set_retryable_tx_info_response_error(&mut clients, &authority_keys);
    // Validators 3 returns tx-cert with epoch 1
    let effects = TransactionEffectsV1 {
        transaction_digest: *cert_epoch_0.digest(),
        status: ExecutionStatus::Failure {
            error: ExecutionFailureStatus::InsufficientGas,
            command: None,
        },
        ..Default::default()
    };
    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter().skip(3),
        Some(cert_epoch_0.inner()),
        TransactionEffects::V1(effects),
        1,
    );

    let agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 1);

    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableTransaction { .. }
            )
        },
        |e| {
            matches!(
                e,
                SuiError::QuorumFailedToGetEffectsQuorumWhenProcessingTransaction { .. }
                    | SuiError::RpcError(..)
            )
        },
    )
    .await;

    println!("Case 6.1 - Retryable Transaction (same as 6.1 but with different tx1 effects)");
    // Val 0 & 1 returns retryable error
    set_retryable_tx_info_response_error(&mut clients, &authority_keys);

    // Validators 2 returns tx-cert and tx-effects with epoch 1
    let effects = TransactionEffectsV1 {
        transaction_digest: *cert_epoch_0.digest(),
        status: ExecutionStatus::Failure {
            error: ExecutionFailureStatus::InsufficientGas,
            command: None,
        },
        ..Default::default()
    };

    let resp = HandleTransactionResponse {
        status: TransactionStatus::Executed(
            None,
            SignedTransactionEffects::new(
                1,
                TransactionEffects::V1(effects.clone()),
                &authority_keys[1].1,
                authority_keys[1].0,
            ),
            TransactionEvents { data: vec![] },
        ),
    };
    clients
        .get_mut(&authority_keys[1].0)
        .unwrap()
        .set_tx_info_response(resp);

    // Validators 3 returns different tx-effects without cert for epoch 1 (simulating byzantine behavior)
    let effects = TransactionEffectsV1 {
        transaction_digest: *cert_epoch_0.digest(),
        status: ExecutionStatus::Failure {
            error: ExecutionFailureStatus::InvalidGasObject,
            command: None,
        },
        ..Default::default()
    };

    let resp = HandleTransactionResponse {
        status: TransactionStatus::Executed(
            None,
            SignedTransactionEffects::new(
                1,
                TransactionEffects::V1(effects.clone()),
                &authority_keys[2].1,
                authority_keys[2].0,
            ),
            TransactionEvents { data: vec![] },
        ),
    };
    clients
        .get_mut(&authority_keys[2].0)
        .unwrap()
        .set_tx_info_response(resp);

    let agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 1);

    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableTransaction { .. }
            )
        },
        |e| {
            matches!(
                e,
                SuiError::QuorumFailedToGetEffectsQuorumWhenProcessingTransaction { .. }
                    | SuiError::RpcError(..)
            )
        },
    )
    .await;

    println!("Case 6.2 - Retryable Transaction (same as 6.1 but with byzantine tx2 effects)");
    // All Validators gives signed-tx2
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx2, 0);

    // Validators now gives valid signed tx2 and we get TxCert2
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    let cert_epoch_0_2 = agg
        .process_transaction(tx2.clone())
        .await
        .unwrap()
        .into_cert_for_testing();

    // Val 0 & 1 returns retryable error
    set_retryable_tx_info_response_error(&mut clients, &authority_keys);

    // Validators 2 returns tx-cert and tx-effects with epoch 1
    let effects = TransactionEffectsV1 {
        transaction_digest: *cert_epoch_0.digest(),
        status: ExecutionStatus::Failure {
            error: ExecutionFailureStatus::InsufficientGas,
            command: None,
        },
        ..Default::default()
    };

    let resp = HandleTransactionResponse {
        status: TransactionStatus::Executed(
            None,
            SignedTransactionEffects::new(
                1,
                TransactionEffects::V1(effects.clone()),
                &authority_keys[1].1,
                authority_keys[1].0,
            ),
            TransactionEvents { data: vec![] },
        ),
    };
    clients
        .get_mut(&authority_keys[1].0)
        .unwrap()
        .set_tx_info_response(resp);

    // Validators 3 returns tx2-effects without cert for epoch 1 (simulating byzantine behavior)
    let effects = TransactionEffectsV1 {
        transaction_digest: *cert_epoch_0_2.digest(),
        status: ExecutionStatus::Failure {
            error: ExecutionFailureStatus::InsufficientGas,
            command: None,
        },
        ..Default::default()
    };

    let resp = HandleTransactionResponse {
        status: TransactionStatus::Executed(
            None,
            SignedTransactionEffects::new(
                1,
                TransactionEffects::V1(effects.clone()),
                &authority_keys[2].1,
                authority_keys[2].0,
            ),
            TransactionEvents { data: vec![] },
        ),
    };
    clients
        .get_mut(&authority_keys[2].0)
        .unwrap()
        .set_tx_info_response(resp);

    let agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 1);

    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableTransaction { .. }
            )
        },
        |e| {
            matches!(
                e,
                SuiError::QuorumFailedToGetEffectsQuorumWhenProcessingTransaction { .. }
                    | SuiError::RpcError(..)
                    | SuiError::ByzantineAuthoritySuspicion { .. }
            )
        },
    )
    .await;

    println!("Case 7 - Retryable Transaction (MissingCommitteeAtEpoch Error)");
    // Validators return tx-cert with epoch 1, client expects 0
    let effects = effects_with_tx(*cert_epoch_1.digest());
    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter(),
        Some(cert_epoch_1.inner()),
        effects,
        1,
    );
    let mut agg = get_genesis_agg(authorities.clone(), clients.clone());

    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableTransaction { .. }
            )
        },
        |e| matches!(e, SuiError::MissingCommitteeAtEpoch(e) if *e == 1),
    )
    .await;

    println!("Case 7.1 - Retryable Transaction (WrongEpoch Error)");
    // Update committee store, now SafeClient will pass
    let committee_1 =
        Committee::new_for_testing_with_normalized_voting_power(1, authorities.clone());
    agg.committee_store
        .insert_new_committee(&committee_1)
        .unwrap();
    assert_resp_err(
        &agg,
        tx.clone(),|e| matches!(e, AggregatorProcessTransactionError::RetryableTransaction { .. }),
        |e| matches!(e, SuiError::WrongEpoch { expected_epoch, actual_epoch } if *expected_epoch == 0 && *actual_epoch == 1)
    )
    .await;

    println!("Case 7.2 - Successful Cert Transaction");
    // Update aggregator committee, and transaction will succeed.
    agg.committee = committee_1;
    agg.process_transaction(tx.clone()).await.unwrap();

    println!("Case 8 - Retryable Transaction (ObjectNotFound Error)");
    // < 2f+1 object not found errors
    set_retryable_tx_info_response_error(&mut clients, &authority_keys);
    for (name, _) in authority_keys.iter().skip(2) {
        clients
            .get_mut(name)
            .unwrap()
            .set_tx_info_response_error(object_not_found_error.clone());
    }
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableTransaction { .. }
            )
        },
        |e| matches!(e, SuiError::UserInputError { .. } | SuiError::RpcError(..)),
    )
    .await;

    // TODO: change to use a move transaction which makes package error more realistic
    println!("Case 8.1 - Retryable Transaction (PackageNotFound Error)");
    // < 2f+1 package not found errors
    for (name, _) in authority_keys.iter().skip(2) {
        clients
            .get_mut(name)
            .unwrap()
            .set_tx_info_response_error(package_not_found_error.clone());
    }
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableTransaction { .. }
            )
        },
        |e| matches!(e, SuiError::UserInputError { .. } | SuiError::RpcError(..)),
    )
    .await;

    println!("Case 8.2 - Retryable Transaction (ObjectNotFound & PackageNotFound Error)");
    // < 2f+1 object + package not found errors
    clients
        .get_mut(&authority_keys[2].0)
        .unwrap()
        .set_tx_info_response_error(package_not_found_error.clone());
    clients
        .get_mut(&authority_keys[3].0)
        .unwrap()
        .set_tx_info_response_error(object_not_found_error.clone());
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableTransaction { .. }
            )
        },
        |e| matches!(e, SuiError::UserInputError { .. } | SuiError::RpcError(..)),
    )
    .await;

    println!("Case 9 - Non-Retryable Transaction (>=2f+1 ObjectNotFound Error)");
    // >= 2f+1 object not found errors
    set_retryable_tx_info_response_error(&mut clients, &authority_keys);
    for (name, _) in authority_keys.iter().skip(1) {
        clients
            .get_mut(name)
            .unwrap()
            .set_tx_info_response_error(object_not_found_error.clone());
    }
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::FatalTransaction { .. }
            )
        },
        |e| matches!(e, SuiError::UserInputError { .. } | SuiError::RpcError(..)),
    )
    .await;

    println!("Case 9.1 - Non-Retryable Transaction (>=2f+1 PackageNotFound Error)");
    // >= 2f+1 package not found errors
    set_retryable_tx_info_response_error(&mut clients, &authority_keys);
    for (name, _) in authority_keys.iter().skip(1) {
        clients
            .get_mut(name)
            .unwrap()
            .set_tx_info_response_error(package_not_found_error.clone());
    }
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::FatalTransaction { .. }
            )
        },
        |e| matches!(e, SuiError::UserInputError { .. } | SuiError::RpcError(..)),
    )
    .await;

    println!("Case 9.2 - Non-Retryable Transaction (>=2f+1 ObjectNotFound+PackageNotFound Error)");
    // < 2f+1 object + package not found errors
    clients
        .get_mut(&authority_keys[1].0)
        .unwrap()
        .set_tx_info_response_error(object_not_found_error.clone());
    clients
        .get_mut(&authority_keys[2].0)
        .unwrap()
        .set_tx_info_response_error(package_not_found_error.clone());
    clients
        .get_mut(&authority_keys[3].0)
        .unwrap()
        .set_tx_info_response_error(object_not_found_error.clone());
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::FatalTransaction { .. }
            )
        },
        |e| matches!(e, SuiError::UserInputError { .. } | SuiError::RpcError(..)),
    )
    .await;
}

#[tokio::test]
async fn test_handle_conflicting_transaction_response() {
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
    let conflicting_object = random_object_ref();
    let tx1 = make_transfer_sui_transaction(
        conflicting_object,
        SuiAddress::default(),
        Some(1),
        sender,
        &sender_kp,
        666, // this is a dummy value which does not matter
    );
    let conflicting_tx2 = make_transfer_sui_transaction(
        conflicting_object,
        SuiAddress::default(),
        Some(2),
        sender,
        &sender_kp,
        666, // this is a dummy value which does not matter
    );
    let conflicting_error = SuiError::ObjectLockConflict {
        obj_ref: conflicting_object,
        pending_transaction: *conflicting_tx2.digest(),
    };
    let retryable_error = SuiError::RpcError("RPC".into(), "Error".into());
    let non_retryable_error = SuiError::ByzantineAuthoritySuspicion {
        authority: authority_keys[0].0,
        reason: "Faulty".into(),
    };
    let object_not_found_error = SuiError::UserInputError {
        error: UserInputError::ObjectNotFound {
            object_id: conflicting_object.0,
            version: Some(conflicting_object.1),
        },
    };

    println!("Case 0 - Retryable Transaction, >= f+1 good stake so ignore conflicting transaction");
    // >= f+1 good stake returned by other validators.
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx1, 0);
    // Val 0 returns conflicting Tx2
    clients
        .get_mut(&authority_keys[0].0)
        .unwrap()
        .set_tx_info_response_error(conflicting_error.clone());
    // Val 1 returns retryable error.
    clients
        .get_mut(&authority_keys[1].0)
        .unwrap()
        .set_tx_info_response_error(retryable_error.clone());
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx1.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableConflictingTransaction {
                    conflicting_tx_digest_to_retry,
                    ..
                } if conflicting_tx_digest_to_retry.is_none()
            )
        },
        |e| {
            matches!(
                e,
                SuiError::ObjectLockConflict { .. } | SuiError::RpcError(..)
            )
        },
    )
    .await;

    println!(
        "Case 1 - Retryable Transaction, state is still retryable so ignore conflicting transaction"
    );
    // Only Val 3 returns conflicting Tx2 and Tx1 is still in retryable state.
    set_retryable_tx_info_response_error(&mut clients, &authority_keys);
    for (name, _) in authority_keys.iter().skip(3) {
        clients
            .get_mut(name)
            .unwrap()
            .set_tx_info_response_error(conflicting_error.clone());
    }
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx1.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableConflictingTransaction {
                    conflicting_tx_digest_to_retry,
                    ..
                } if conflicting_tx_digest_to_retry.is_none()
            )
        },
        |e| {
            matches!(
                e,
                SuiError::ObjectLockConflict { .. } | SuiError::RpcError(..)
            )
        },
    )
    .await;

    println!("Case 2 - Non-retryable Tx but Retryable Conflicting Transaction");
    // Validators return >= f+1 conflicting Tx2
    set_retryable_tx_info_response_error(&mut clients, &authority_keys);
    for (name, _) in authority_keys.iter().skip(1) {
        clients
            .get_mut(name)
            .unwrap()
            .set_tx_info_response_error(conflicting_error.clone());
    }

    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx1.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableConflictingTransaction {
                    conflicting_tx_digest_to_retry,
                    ..
                } if *conflicting_tx_digest_to_retry == Some(*conflicting_tx2.digest())
            )
        },
        |e| {
            matches!(
                e,
                SuiError::ObjectLockConflict { .. } | SuiError::RpcError(..)
            )
        },
    )
    .await;

    println!("Case 3 - Non-retryable Tx due to client double spend");
    // Validators return >= f+1 conflicting Tx2 & >= f+1 good stake for Tx1
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx1, 0);
    for (name, _) in authority_keys.iter().skip(2) {
        clients
            .get_mut(name)
            .unwrap()
            .set_tx_info_response_error(conflicting_error.clone());
    }

    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx1.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::FatalConflictingTransaction { .. }
            )
        },
        |e| matches!(e, SuiError::ObjectLockConflict { .. }),
    )
    .await;

    println!("Case 3 - Non-retryable Tx (Mixed Response - 2 conflicts, 1 signed, 1 non-retryable)");
    // Validator 1 returns a signed tx1
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx1, 0);
    // Validator 2 returns a conflicting tx2
    clients
        .get_mut(&authority_keys[1].0)
        .unwrap()
        .set_tx_info_response_error(conflicting_error.clone());
    // Validator 3 returns a conflicting tx3
    let conflicting_tx3 = make_transfer_sui_transaction(
        conflicting_object,
        SuiAddress::default(),
        Some(3),
        sender,
        &sender_kp,
        666, // this is a dummy value which does not matter
    );
    let conflicting_error_2 = SuiError::ObjectLockConflict {
        obj_ref: conflicting_object,
        pending_transaction: *conflicting_tx3.digest(),
    };
    clients
        .get_mut(&authority_keys[2].0)
        .unwrap()
        .set_tx_info_response_error(conflicting_error_2.clone());
    // Validator 4 returns a nonretryable error
    clients
        .get_mut(&authority_keys[3].0)
        .unwrap()
        .set_tx_info_response_error(non_retryable_error.clone());

    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx1.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::FatalConflictingTransaction { .. }
            )
        },
        |e| {
            matches!(
                e,
                SuiError::ObjectLockConflict { .. } | SuiError::ByzantineAuthoritySuspicion { .. }
            )
        },
    )
    .await;

    println!("Case 3.1 - Non-retryable Tx (Mixed Response - 1 conflict, 1 signed, 1 non-retryable, 1 retryable)");
    // Validator 1 returns a signed tx1
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx1, 0);
    // Validator 2 returns a conflicting tx2
    clients
        .get_mut(&authority_keys[1].0)
        .unwrap()
        .set_tx_info_response_error(conflicting_error.clone());
    // Validator 3 returns a conflicting tx3
    let conflicting_tx3 = make_transfer_sui_transaction(
        conflicting_object,
        SuiAddress::default(),
        Some(3),
        sender,
        &sender_kp,
        666, // this is a dummy value which does not matter
    );
    let conflicting_error_2 = SuiError::ObjectLockConflict {
        obj_ref: conflicting_object,
        pending_transaction: *conflicting_tx3.digest(),
    };
    clients
        .get_mut(&authority_keys[2].0)
        .unwrap()
        .set_tx_info_response_error(conflicting_error_2.clone());
    // Validator 4 returns a ObjectNotFound error
    clients
        .get_mut(&authority_keys[3].0)
        .unwrap()
        .set_tx_info_response_error(object_not_found_error.clone());

    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx1.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::FatalConflictingTransaction { .. }
            )
        },
        |e| {
            matches!(
                e,
                SuiError::ObjectLockConflict { .. } | SuiError::UserInputError { .. }
            )
        },
    )
    .await;

    println!(
        "Case 3.2 - Non-retryable Tx (Mixed Response - 1 conflict, 1 signed, 1 non-retryable, 1 ObjectNotFoundError)"
    );
    // Validator 1 returns a signed tx1
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx1, 0);
    // Validator 2 returns a conflicting tx2
    clients
        .get_mut(&authority_keys[1].0)
        .unwrap()
        .set_tx_info_response_error(conflicting_error.clone());
    // Validator 3 returns a nonretryable error
    clients
        .get_mut(&authority_keys[2].0)
        .unwrap()
        .set_tx_info_response_error(non_retryable_error.clone());
    // Validator 4 returns a ObjectNotFound error
    clients
        .get_mut(&authority_keys[3].0)
        .unwrap()
        .set_tx_info_response_error(object_not_found_error.clone());

    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        tx1.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::FatalConflictingTransaction { .. }
            )
        },
        |e| {
            matches!(
                e,
                SuiError::ObjectLockConflict { .. }
                    | SuiError::UserInputError { .. }
                    | SuiError::ByzantineAuthoritySuspicion { .. }
            )
        },
    )
    .await;

    println!("Case 4 - Successful Conflicting Transaction with Cert");
    // All Validators gives signed-tx
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx1, 0);

    // Validators now gives valid signed tx and we get TxCert
    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    let cert_epoch_0 = agg
        .process_transaction(tx1.clone())
        .await
        .unwrap()
        .into_cert_for_testing();

    // Val-1 returns conflicting tx2; Val2-3 returns retryable error
    set_retryable_tx_info_response_error(&mut clients, &authority_keys);
    let (name_1, _) = &authority_keys[1];
    clients
        .get_mut(name_1)
        .unwrap()
        .set_tx_info_response_error(conflicting_error.clone());

    // Val-0 returns cert
    let (name_0, key_0) = &authority_keys[0];
    let effects = TransactionEffectsV1 {
        transaction_digest: *cert_epoch_0.digest(),
        ..Default::default()
    };
    let resp = HandleTransactionResponse {
        status: TransactionStatus::Executed(
            Some(cert_epoch_0.auth_sig().clone()),
            sign_tx_effects(TransactionEffects::V1(effects.clone()), 0, *name_0, key_0),
            TransactionEvents { data: vec![] },
        ),
    };
    clients.get_mut(name_0).unwrap().set_tx_info_response(resp);

    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    // We have a valid cert because val-0 has it
    agg.process_transaction(tx1.clone()).await.unwrap();

    println!("Case 5 - Retryable Transaction (MissingCommitteeAtEpoch Error)");
    // Validators return signed-tx with epoch 1
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx1, 1);

    // Val-1 sends conflicting tx2
    let (name_1, _) = &authority_keys[1];
    clients
        .get_mut(name_1)
        .unwrap()
        .set_tx_info_response_error(conflicting_error.clone());

    let agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 1);
    let cert_epoch_1 = agg
        .process_transaction(tx1.clone())
        .await
        .unwrap()
        .into_cert_for_testing();

    // Validators have moved to epoch 2 and return tx-effects with epoch 2, client expects 1
    let effects = TransactionEffectsV1 {
        transaction_digest: *cert_epoch_1.digest(),
        ..Default::default()
    };
    set_tx_info_response_with_cert_and_effects(
        &mut clients,
        authority_keys.iter(),
        None,
        TransactionEffects::V1(effects),
        2,
    );

    // Val-1 sends conflicting tx2
    let (name_1, _) = &authority_keys[1];
    clients
        .get_mut(name_1)
        .unwrap()
        .set_tx_info_response_error(conflicting_error.clone());

    let mut agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 1);
    assert_resp_err(
        &agg,
        tx1.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableConflictingTransaction { .. }
            )
        },
        |e| {
            matches!(
                e,
                SuiError::MissingCommitteeAtEpoch(..) | SuiError::ObjectLockConflict { .. }
            )
        },
    )
    .await;

    println!("Case 5.1 - Retryable Transaction (WrongEpoch Error)");
    // Update committee store to epoch 2, now SafeClient will pass
    let committee_2 =
        Committee::new_for_testing_with_normalized_voting_power(2, authorities.clone());
    agg.committee_store
        .insert_new_committee(&committee_2)
        .unwrap();
    assert_resp_err(
        &agg,
        tx1.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableConflictingTransaction { .. }
            )
        },
        |e| {
            matches!(
                e,
                SuiError::WrongEpoch { .. } | SuiError::ObjectLockConflict { .. }
            )
        },
    )
    .await;

    println!("Case 5.2 - Successful Cert Transaction");
    // Update aggregator committee to epoch 2, and transaction will succeed.
    agg.committee = committee_2;
    agg.process_transaction(tx1.clone()).await.unwrap();
}

#[tokio::test]
async fn test_handle_overload_response() {
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
    let gas_object = random_object_ref();
    let txn = make_transfer_sui_transaction(
        gas_object,
        SuiAddress::default(),
        None,
        sender,
        &sender_kp,
        666, // this is a dummy value which does not matter
    );

    let overload_error = SuiError::TooManyTransactionsPendingExecution {
        queue_len: 100,
        threshold: 100,
    };
    let rpc_error = SuiError::RpcError("RPC".into(), "Error".into());

    // Have 2f + 1 validators return the overload error and we should get the `SystemOverload` error.
    set_retryable_tx_info_response_error(&mut clients, &authority_keys);
    set_tx_info_response_with_error(&mut clients, authority_keys.iter().skip(1), overload_error);

    let agg = get_genesis_agg(authorities.clone(), clients.clone());
    assert_resp_err(
        &agg,
        txn.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::SystemOverload {
                    overloaded_stake,
                    ..
                } if *overloaded_stake == 7500
            )
        },
        |e| {
            matches!(
                e,
                SuiError::TooManyTransactionsPendingExecution { .. } | SuiError::RpcError(..)
            )
        },
    )
    .await;

    // Change one of the valdiators' errors to RPC error so the system is considered not overloaded now and a `RetryableTransaction`
    // should be returned.
    clients
        .get_mut(&authority_keys[1].0)
        .unwrap()
        .set_tx_info_response_error(rpc_error);

    let agg = get_genesis_agg(authorities.clone(), clients.clone());

    assert_resp_err(
        &agg,
        txn.clone(),
        |e| {
            matches!(
                e,
                AggregatorProcessTransactionError::RetryableTransaction { .. }
            )
        },
        |e| {
            matches!(
                e,
                SuiError::TooManyTransactionsPendingExecution { .. } | SuiError::RpcError(..)
            )
        },
    )
    .await;
}

#[tokio::test]
async fn test_byzantine_authority_sig_aggregation() {
    telemetry_subscribers::init_for_testing();
    // For 4 validators, we need 2f+1 = 3 for quorum for signing a sender signed tx.
    assert!(run_aggregator(1, 4).await.is_ok());
    assert!(run_aggregator(2, 4).await.is_err());

    // For 6 validators, voting power normaliziation in test Committee construction
    // will result in 2f+1 = 4 for quorum for signing a sender signed tx.
    assert!(run_aggregator(1, 6).await.is_ok());
    assert!(run_aggregator(2, 6).await.is_ok());
    assert!(run_aggregator(3, 6).await.is_err());

    // For 4 validators, we need 2f+1 = 3 for quorum for signing transaction effects.
    assert!(process_with_cert(1, 4).await.is_ok());

    // For 6 validators, we need 2f+1 = 5 for quorum for signing transaction effects.
    assert!(process_with_cert(1, 6).await.is_ok());

    // For 12 validators, we need 2f+1 = 9 for quorum for signing transaction effects.
    assert!(process_with_cert(1, 12).await.is_ok());
    assert!(process_with_cert(2, 12).await.is_ok());
    assert!(process_with_cert(3, 12).await.is_ok());
}

#[tokio::test]
#[should_panic]
async fn test_fork_panic_process_cert_6_auths() {
    telemetry_subscribers::init_for_testing();
    let _ = process_with_cert(3, 6).await;
}

#[tokio::test]
#[should_panic]
async fn test_fork_panic_process_cert_4_auths() {
    telemetry_subscribers::init_for_testing();
    let _ = process_with_cert(2, 4).await;
}

// Aggregator aggregate signatures from authorities and process the transaction as signed.
// Test [fn handle_transaction_response_with_signed].
async fn run_aggregator(
    num_byzantines: u8,
    num_authorities: u8,
) -> Result<ProcessTransactionResult, AggregatorProcessTransactionError> {
    let tx = create_fake_transaction();
    let mut authorities = BTreeMap::new();
    let mut clients = BTreeMap::new();
    let mut authority_keys = Vec::new();
    let mut byzantines = Vec::new();

    // Assign a few authorities as byzantines represented in a list of pubkeys.
    for i in 0..num_byzantines {
        let byzantine =
            get_key_pair_from_rng::<AuthorityKeyPair, StdRng>(&mut StdRng::from_seed([i; 32]))
                .1
                .public()
                .into();
        byzantines.push(byzantine);
    }

    // Set up authorities and their clients.
    for i in 0..num_authorities {
        let (_, sec): (_, AuthorityKeyPair) =
            get_key_pair_from_rng(&mut StdRng::from_seed([i; 32]));
        let name: AuthorityName = sec.public().into();
        authorities.insert(name, 1);
        authority_keys.push((name, sec));
        clients.insert(name, HandleTransactionTestAuthorityClient::new());
    }

    for (name, secret) in &authority_keys {
        let auth_signature = if byzantines.contains(name) {
            // If the authority is a byzantine authority, create an invalid auth signature.
            AuthoritySignInfo::new(
                0,
                tx.clone().data(),
                Intent::sui_app(IntentScope::ProofOfPossession), // bad intent
                *name,
                secret,
            )
        } else {
            // Otherwise, create a valid auth signature on sender signed transaction.
            AuthoritySignInfo::new(
                0,
                tx.clone().data(),
                Intent::sui_app(IntentScope::SenderSignedTransaction),
                *name,
                secret,
            )
        };
        // For each client, set the response with the correspond good/bad auth signatures.
        let resp = HandleTransactionResponse {
            status: TransactionStatus::Signed(auth_signature),
        };
        clients.get_mut(name).unwrap().set_tx_info_response(resp);
    }

    let agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 0);
    agg.process_transaction(tx.clone()).await
}

// Aggregator aggregate signatures from authorities and process the transaction as executed.
// Test [fn handle_transaction_response_with_executed].
async fn process_with_cert(
    num_byzantines: u8,
    num_authorities: u8,
) -> Result<ProcessTransactionResult, AggregatorProcessTransactionError> {
    let tx = create_fake_transaction();
    let mut authorities = BTreeMap::new();
    let mut clients = BTreeMap::new();
    let mut authority_keys = Vec::new();
    let mut byzantines = Vec::new();

    // Assign a few authorities as byzantines represented in a list of pubkeys.
    for i in 0..num_byzantines {
        let byzantine =
            get_key_pair_from_rng::<AuthorityKeyPair, StdRng>(&mut StdRng::from_seed([i; 32]))
                .1
                .public()
                .into();
        byzantines.push(byzantine);
    }

    // Set up authorities and their clients.
    for i in 0..num_authorities {
        let (_, sec): (_, AuthorityKeyPair) =
            get_key_pair_from_rng(&mut StdRng::from_seed([i; 32]));
        let name: AuthorityName = sec.public().into();
        authorities.insert(name, 1);
        authority_keys.push((name, sec));
        clients.insert(name, HandleTransactionTestAuthorityClient::new());
    }
    set_tx_info_response_with_signed_tx(&mut clients, &authority_keys, &tx, 0);

    // Process the transaction first with an execution result as signed.
    let agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 0);
    let cert = agg
        .process_transaction(tx.clone())
        .await
        .unwrap()
        .into_cert_for_testing();
    let effects = effects_with_tx(*cert.digest());

    for (name, secret) in &authority_keys {
        let auth_signature = if byzantines.contains(name) {
            // If the authority is a byzantine authority, create an invalid auth signature.
            AuthoritySignInfo::new(
                0,
                &effects.clone(),
                Intent::sui_app(IntentScope::ProofOfPossession), // bad intent
                *name,
                secret,
            )
        } else {
            // Otherwise, create a valid auth signature on transaction effects.
            AuthoritySignInfo::new(
                0,
                &effects.clone(),
                Intent::sui_app(IntentScope::TransactionEffects),
                *name,
                secret,
            )
        };
        // Set the client response as executed with the signed transaction effects
        // with corresponding good/bad auth signature.
        let resp = HandleTransactionResponse {
            status: TransactionStatus::Executed(
                None,
                SignedTransactionEffects::new_from_data_and_sig(effects.clone(), auth_signature),
                TransactionEvents { data: vec![] },
            ),
        };

        clients.get_mut(name).unwrap().set_tx_info_response(resp);
    }
    let agg = get_agg_at_epoch(authorities.clone(), clients.clone(), 0);
    agg.process_transaction(tx.clone()).await
}

async fn assert_resp_err<E, F>(
    agg: &AuthorityAggregator<HandleTransactionTestAuthorityClient>,
    tx: VerifiedTransaction,
    agg_err_checker: E,
    sui_err_checker: F,
) where
    E: Fn(&AggregatorProcessTransactionError) -> bool,
    F: Fn(&SuiError) -> bool,
{
    match agg.process_transaction(tx).await {
        Err(received_agg_err) if agg_err_checker(&received_agg_err) => match received_agg_err {
            AggregatorProcessTransactionError::RetryableConflictingTransaction {
                errors,
                conflicting_tx_digest_to_retry: _,
                conflicting_tx_digests,
            } => {
                assert!(!conflicting_tx_digests.is_empty());
                assert!(errors.iter().map(|e| &e.0).all(sui_err_checker));
            }

            AggregatorProcessTransactionError::FatalConflictingTransaction {
                errors,
                conflicting_tx_digests,
            } => {
                assert!(!conflicting_tx_digests.is_empty());
                assert!(errors.iter().map(|e| &e.0).all(sui_err_checker));
            }

            AggregatorProcessTransactionError::RetryableTransaction { errors } => {
                assert!(errors.iter().map(|e| &e.0).all(sui_err_checker));
            }

            AggregatorProcessTransactionError::FatalTransaction { errors } => {
                assert!(errors.iter().map(|e| &e.0).all(sui_err_checker));
            }

            AggregatorProcessTransactionError::SystemOverload { errors, .. } => {
                assert!(errors.iter().map(|e| &e.0).all(sui_err_checker));
            }
        },
        Err(received_agg_err) => {
            assert!(
                agg_err_checker(&received_agg_err),
                "Unexpected AggregatorProcessTransactionError: {received_agg_err:?}"
            );
        }
        Ok(_) => {
            panic!("Expected AggregatorProcessTransactionError but got Ok");
        }
    }
}

fn set_tx_info_response_with_cert_and_effects<'a>(
    clients: &mut BTreeMap<AuthorityName, HandleTransactionTestAuthorityClient>,
    authority_keys: impl Iterator<Item = &'a (AuthorityName, AuthorityKeyPair)>,
    cert: Option<&CertifiedTransaction>,
    effects: TransactionEffects,
    epoch: EpochId,
) {
    for (name, key) in authority_keys {
        let resp = HandleTransactionResponse {
            status: TransactionStatus::Executed(
                cert.map(|c| c.auth_sig().clone()),
                SignedTransactionEffects::new(epoch, effects.clone(), key, *name),
                TransactionEvents { data: vec![] },
            ),
        };
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

        let resp = HandleTransactionResponse {
            status: TransactionStatus::Signed(signed_tx.into_sig()),
        };
        clients.get_mut(name).unwrap().set_tx_info_response(resp);
    }
}

fn set_retryable_tx_info_response_error(
    clients: &mut BTreeMap<AuthorityName, HandleTransactionTestAuthorityClient>,
    authority_keys: &[(AuthorityName, AuthorityKeyPair)],
) {
    let error = SuiError::RpcError("RPC".into(), "Error".into());
    set_tx_info_response_with_error(clients, authority_keys.iter(), error);
}

fn set_tx_info_response_with_error<'a>(
    clients: &mut BTreeMap<AuthorityName, HandleTransactionTestAuthorityClient>,
    authority_keys: impl Iterator<Item = &'a (AuthorityName, AuthorityKeyPair)>,
    error: SuiError,
) {
    for (name, _) in authority_keys {
        clients
            .get_mut(name)
            .unwrap()
            .set_tx_info_response_error(error.clone());
    }
}
