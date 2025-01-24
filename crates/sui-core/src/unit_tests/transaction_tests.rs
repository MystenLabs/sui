// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::mock_consensus::with_block_status;
use consensus_core::{BlockRef, BlockStatus};
use fastcrypto::{ed25519::Ed25519KeyPair, traits::KeyPair};
use fastcrypto_zkp::bn254::zk_login::{parse_jwks, OIDCProvider, ZkLoginInputs};
use move_core_types::ident_str;
use rand::{rngs::StdRng, SeedableRng};
use shared_crypto::intent::{Intent, IntentMessage};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::ops::Deref;
use sui_types::crypto::{PublicKey, SuiSignature, ToFromBytes, ZkLoginPublicIdentifier};
use sui_types::messages_grpc::HandleSoftBundleCertificatesRequestV3;
use sui_types::utils::get_one_zklogin_inputs;
use sui_types::{
    authenticator_state::ActiveJwk,
    base_types::dbg_addr,
    crypto::{get_key_pair, AccountKeyPair, Signature, SuiKeyPair},
    error::{SuiError, UserInputError},
    messages_consensus::ConsensusDeterminedVersionAssignments,
    multisig::{MultiSig, MultiSigPublicKey},
    signature::GenericSignature,
    transaction::{
        AuthenticatorStateUpdate, GenesisTransaction, TransactionDataAPI, TransactionKind,
    },
    utils::{load_test_vectors, to_sender_signed_transaction},
    zk_login_authenticator::ZkLoginAuthenticator,
    zk_login_util::DEFAULT_JWK_BYTES,
};

use crate::authority::authority_test_utils::send_batch_consensus_no_execution;
use crate::authority::authority_tests::{call_move_, create_gas_objects, publish_object_basics};
use crate::consensus_adapter::consensus_tests::make_consensus_adapter_for_test;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::SUI_SYSTEM_PACKAGE_ID;

use sui_macros::sim_test;
macro_rules! assert_matches {
    ($expression:expr, $pattern:pat $(if $guard: expr)?) => {
        match $expression {
            $pattern $(if $guard)? => {}
            ref e => panic!(
                "assertion failed: `(left == right)` \
                 (left: `{:?}`, right: `{:?}`)",
                e,
                stringify!($pattern $(if $guard)?)
            ),
        }
    };
}

use crate::{
    authority_client::{AuthorityAPI, NetworkAuthorityClient},
    authority_server::{AuthorityServer, AuthorityServerHandle},
    stake_aggregator::{InsertResult, StakeAggregator},
};

use super::*;
use fastcrypto::traits::AggregateAuthenticator;
use sui_types::digests::ConsensusCommitDigest;
use sui_types::messages_consensus::{
    ConsensusCommitPrologue, ConsensusCommitPrologueV2, ConsensusCommitPrologueV3,
};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;

pub use crate::authority::authority_test_utils::init_state_with_ids;

#[sim_test]
async fn test_handle_transfer_transaction_bad_signature() {
    do_transaction_test(
        1,
        |_| {},
        |mut_tx| {
            let (_unknown_address, unknown_key): (_, AccountKeyPair) = get_key_pair();
            let data = mut_tx.data_mut_for_testing();
            *data.tx_signatures_mut_for_testing() =
                vec![Signature::new_secure(data.intent_message(), &unknown_key).into()];
        },
        |err| {
            assert_matches!(err, SuiError::SignerSignatureAbsent { .. });
        },
    )
    .await;
}

#[sim_test]
async fn test_handle_transfer_transaction_no_signature() {
    do_transaction_test(
        1,
        |_| {},
        |tx| {
            *tx.data_mut_for_testing().tx_signatures_mut_for_testing() = vec![];
        },
        |err| {
            assert_matches!(
                err,
                SuiError::SignerSignatureNumberMismatch {
                    expected: 1,
                    actual: 0
                }
            );
        },
    )
    .await;
}

#[sim_test]
async fn test_handle_transfer_transaction_extra_signature() {
    do_transaction_test(
        1,
        |_| {},
        |tx| {
            let sigs = tx.data_mut_for_testing().tx_signatures_mut_for_testing();
            sigs.push(sigs[0].clone());
        },
        |err| {
            assert_matches!(
                err,
                SuiError::SignerSignatureNumberMismatch {
                    expected: 1,
                    actual: 2
                }
            );
        },
    )
    .await;
}

#[sim_test]
async fn test_empty_gas_data() {
    do_transaction_test_skip_cert_checks(
        0,
        |tx| {
            tx.gas_data_mut().payment = vec![];
        },
        |_| {},
        |err| {
            assert_matches!(
                err,
                SuiError::UserInputError {
                    error: UserInputError::MissingGasPayment
                }
            );
        },
    )
    .await;
}

#[sim_test]
async fn test_duplicate_gas_data() {
    do_transaction_test_skip_cert_checks(
        0,
        |tx| {
            let gas_data = tx.gas_data_mut();
            let new_gas = gas_data.payment[0];
            gas_data.payment.push(new_gas);
        },
        |_| {},
        |err| {
            assert_matches!(
                err,
                SuiError::UserInputError {
                    error: UserInputError::MutableObjectUsedMoreThanOnce { .. }
                }
            );
        },
    )
    .await;
}

#[sim_test]
async fn test_gas_wrong_owner_matches_sender() {
    do_transaction_test(
        1,
        |tx| {
            let gas_data = tx.gas_data_mut();
            let (new_addr, _): (_, AccountKeyPair) = get_key_pair();
            gas_data.owner = new_addr;
            *tx.sender_mut_for_testing() = new_addr;
        },
        |_| {},
        |err| {
            assert_matches!(err, SuiError::SignerSignatureAbsent { .. });
        },
    )
    .await;
}

#[sim_test]
async fn test_gas_wrong_owner() {
    do_transaction_test(
        1,
        |tx| {
            let gas_data = tx.gas_data_mut();
            let (new_addr, _): (_, AccountKeyPair) = get_key_pair();
            gas_data.owner = new_addr;
        },
        |_| {},
        |err| {
            assert_matches!(
                err,
                SuiError::SignerSignatureNumberMismatch {
                    expected: 2,
                    actual: 1
                }
            );
        },
    )
    .await;
}

#[sim_test]
async fn test_user_sends_genesis_transaction() {
    test_user_sends_system_transaction_impl(TransactionKind::Genesis(GenesisTransaction {
        objects: vec![],
    }))
    .await;
}

#[tokio::test]
async fn test_user_sends_consensus_commit_prologue() {
    test_user_sends_system_transaction_impl(TransactionKind::ConsensusCommitPrologue(
        ConsensusCommitPrologue {
            epoch: 0,
            round: 0,
            commit_timestamp_ms: 42,
        },
    ))
    .await;
}

#[tokio::test]
async fn test_user_sends_consensus_commit_prologue_v2() {
    test_user_sends_system_transaction_impl(TransactionKind::ConsensusCommitPrologueV2(
        ConsensusCommitPrologueV2 {
            epoch: 0,
            round: 0,
            commit_timestamp_ms: 42,
            consensus_commit_digest: ConsensusCommitDigest::default(),
        },
    ))
    .await;
}

#[tokio::test]
async fn test_user_sends_consensus_commit_prologue_v3() {
    test_user_sends_system_transaction_impl(TransactionKind::ConsensusCommitPrologueV3(
        ConsensusCommitPrologueV3 {
            epoch: 0,
            round: 0,
            sub_dag_index: None,
            commit_timestamp_ms: 42,
            consensus_commit_digest: ConsensusCommitDigest::default(),
            consensus_determined_version_assignments:
                ConsensusDeterminedVersionAssignments::empty_for_testing(),
        },
    ))
    .await;
}

#[tokio::test]
async fn test_user_sends_change_epoch_transaction() {
    test_user_sends_system_transaction_impl(TransactionKind::ChangeEpoch(ChangeEpoch {
        epoch: 0,
        protocol_version: ProtocolVersion::MIN,
        storage_charge: 0,
        computation_charge: 0,
        storage_rebate: 0,
        non_refundable_storage_fee: 0,
        epoch_start_timestamp_ms: 0,
        system_packages: vec![],
    }))
    .await;
}

#[tokio::test]
async fn test_user_sends_end_of_epoch_transaction() {
    test_user_sends_system_transaction_impl(TransactionKind::EndOfEpochTransaction(vec![])).await;
}

async fn test_user_sends_system_transaction_impl(transaction_kind: TransactionKind) {
    do_transaction_test_skip_cert_checks(
        0,
        |tx| {
            *tx.kind_mut() = transaction_kind.clone();
        },
        |_| {},
        |err| {
            assert_matches!(
                err,
                SuiError::UserInputError {
                    error: UserInputError::Unsupported { .. }
                }
            );
        },
    )
    .await;
}

pub fn init_transfer_transaction(
    pre_sign_mutations: impl Fn(&mut TransactionData),
    sender: SuiAddress,
    secret: &AccountKeyPair,
    recipient: SuiAddress,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
    gas_budget: u64,
    gas_price: u64,
) -> Transaction {
    let mut data = TransactionData::new_transfer(
        recipient,
        object_ref,
        sender,
        gas_object_ref,
        gas_budget,
        gas_price,
    );
    pre_sign_mutations(&mut data);
    to_sender_signed_transaction(data, secret)
}

pub fn init_move_call_transaction(
    pre_sign_mutations: impl Fn(&mut TransactionData),
    sender: SuiAddress,
    secret: &AccountKeyPair,
    gas_object_ref: ObjectRef,
    gas_budget: u64,
    gas_price: u64,
) -> Transaction {
    let mut data = TransactionData::new_move_call(
        sender,
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.into(),
        ident_str!("request_add_validator").to_owned(),
        vec![],
        gas_object_ref,
        vec![CallArg::SUI_SYSTEM_MUT],
        gas_budget,
        gas_price,
    )
    .unwrap();
    pre_sign_mutations(&mut data);
    to_sender_signed_transaction(data, secret)
}

async fn do_transaction_test_skip_cert_checks(
    expected_sig_errors: u64,
    pre_sign_mutations: impl Fn(&mut TransactionData),
    post_sign_mutations: impl Fn(&mut Transaction),
    err_check: impl Fn(&SuiError),
) {
    do_transaction_test_impl(
        expected_sig_errors,
        false,
        pre_sign_mutations,
        post_sign_mutations,
        err_check,
    )
    .await
}

async fn do_transaction_test(
    expected_sig_errors: u64,
    pre_sign_mutations: impl Fn(&mut TransactionData),
    post_sign_mutations: impl Fn(&mut Transaction),
    err_check: impl Fn(&SuiError),
) {
    do_transaction_test_impl(
        expected_sig_errors,
        true,
        pre_sign_mutations,
        post_sign_mutations,
        err_check,
    )
    .await
}

async fn do_transaction_test_impl(
    _expected_sig_errors: u64,
    check_forged_cert: bool,
    pre_sign_mutations: impl Fn(&mut TransactionData),
    post_sign_mutations: impl Fn(&mut Transaction),
    err_check: impl Fn(&SuiError),
) {
    telemetry_subscribers::init_for_testing();
    let (sender1, sender_key1): (_, AccountKeyPair) = get_key_pair();
    let (sender2, sender_key2): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id1 = ObjectID::random();
    let gas_object_id2 = ObjectID::random();
    let authority_state = init_state_with_ids(vec![
        (sender1, object_id),
        (sender1, gas_object_id1),
        (sender2, gas_object_id2),
    ])
    .await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let object = authority_state.get_object(&object_id).await.unwrap();
    let gas_object1 = authority_state.get_object(&gas_object_id1).await.unwrap();
    let gas_object2 = authority_state.get_object(&gas_object_id2).await.unwrap();

    // Execute the test with two transactions, one transfer and one move call.
    // The move call contains access to a shared object.
    // We test both txs and expect the same error.
    let mut transfer_transaction = init_transfer_transaction(
        &pre_sign_mutations,
        sender1,
        &sender_key1,
        recipient,
        object.compute_object_reference(),
        gas_object1.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    let mut move_call_transaction = init_move_call_transaction(
        &pre_sign_mutations,
        sender2,
        &sender_key2,
        gas_object2.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    let server = AuthorityServer::new_for_test(authority_state.clone());

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Some(
            authority_state
                .config
                .network_key_pair()
                .public()
                .to_owned(),
        ),
    )
    .await
    .unwrap();

    post_sign_mutations(&mut transfer_transaction);
    post_sign_mutations(&mut move_call_transaction);
    let socket_addr = make_socket_addr();

    let transactions = vec![transfer_transaction, move_call_transaction];
    for transaction in &transactions {
        let err = client
            .handle_transaction(transaction.clone(), Some(socket_addr))
            .await
            .unwrap_err();
        err_check(&err);
    }

    check_locks(authority_state.clone(), vec![object_id]).await;

    // now verify that the same transactions are rejected if false certificates are somehow formed and sent
    if check_forged_cert {
        let epoch_store = authority_state.epoch_store_for_testing();
        for transaction in transactions {
            let signed_transaction = VerifiedSignedTransaction::new(
                epoch_store.epoch(),
                VerifiedTransaction::new_unchecked(transaction),
                authority_state.name,
                &*authority_state.secret,
            );
            let mut agg = StakeAggregator::new(epoch_store.committee().clone());

            let InsertResult::QuorumReached(cert_sig) =
                agg.insert(signed_transaction.clone().into())
            else {
                panic!("quorum expected");
            };

            let plain_tx = signed_transaction.into_inner();

            let ct = CertifiedTransaction::new_from_data_and_sig(plain_tx.into_data(), cert_sig);

            let err = client
                .handle_certificate_v2(ct.clone(), Some(socket_addr))
                .await
                .unwrap_err();
            err_check(&err);
            epoch_store.clear_signature_cache();
            let err = client
                .handle_certificate_v2(ct.clone(), Some(socket_addr))
                .await
                .unwrap_err();
            err_check(&err);

            // Additionally, if the tx contains access to shared objects, check if Soft Bundle handler returns the same error.
            if ct.contains_shared_object() {
                epoch_store.clear_signature_cache();
                let err = client
                    .handle_soft_bundle_certificates_v3(
                        HandleSoftBundleCertificatesRequestV3 {
                            certificates: vec![ct.clone()],
                            wait_for_effects: true,
                            include_events: false,
                            include_auxiliary_data: false,
                            include_input_objects: false,
                            include_output_objects: false,
                        },
                        Some(socket_addr),
                    )
                    .await
                    .unwrap_err();
                err_check(&err);
            }
        }
    }
}

#[sim_test]
async fn test_zklogin_transfer_with_bad_ephemeral_sig() {
    do_zklogin_transaction_test(
        1,
        |_| {},
        |tx| {
            let data = tx.data_mut_for_testing();
            let intent_message = data.intent_message().clone();
            let sigs = data.tx_signatures_mut_for_testing();
            let GenericSignature::ZkLoginAuthenticator(zklogin) = sigs.get_mut(0).unwrap() else {
                panic!();
            };

            let (_unknown_address, unknown_key): (_, AccountKeyPair) = get_key_pair();
            let sig = Signature::new_secure(&intent_message, &unknown_key);
            *zklogin.user_signature_mut_for_testing() = sig;
        },
    )
    .await;
}
#[sim_test]
async fn test_zklogin_transfer_with_large_address_seed() {
    telemetry_subscribers::init_for_testing();
    let (
        object_ids,
        gas_object_ids,
        authority_state,
        _epoch_store,
        _,
        _,
        _server,
        client,
        _senders,
        _multisig_pk,
    ) = setup_zklogin_network(|_| {}).await;

    let ephemeral_key = Ed25519KeyPair::generate(&mut StdRng::from_seed([3; 32]));

    let large_address_seed =
        num_bigint::BigInt::from_bytes_be(num_bigint::Sign::Plus, &[1; 33]).to_string();
    let zklogin = ZkLoginInputs::from_json("{\"proofPoints\":{\"a\":[\"7351610957585487046328875967050889651854514987235893782501043846344306437586\",\"15901581830174345085102528605366245320934422564305327249129736514949843983391\",\"1\"],\"b\":[[\"8511334686125322419369086121569737536249817670014553268281989325333085952301\",\"4879445774811020644521006463993914729416121646921376735430388611804034116132\"],[\"17435652898871739253945717312312680537810513841582909477368887889905134847157\",\"14885460127400879557124294989610467103783286587437961743305395373299049315863\"],[\"1\",\"0\"]],\"c\":[\"18935582624804960299209074901817240117999581542763303721451852621662183299378\",\"5367019427921492326304024952457820199970536888356564030410757345854117465786\",\"1\"]},\"issBase64Details\":{\"value\":\"wiaXNzIjoiaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyIiw\",\"indexMod4\":2},\"headerBase64\":\"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEifQ\"}", &large_address_seed).unwrap();
    let sender = SuiAddress::generate(StdRng::from_seed([3; 32]));
    let recipient = dbg_addr(2);

    let tx = init_zklogin_transfer(
        &authority_state,
        object_ids[2],
        gas_object_ids[2],
        recipient,
        sender,
        |_| {},
        &ephemeral_key,
        &zklogin,
    )
    .await;

    assert!(client
        .handle_transaction(tx, Some(make_socket_addr()))
        .await
        .is_err());
}

#[sim_test]
async fn zklogin_test_caching_scenarios() {
    telemetry_subscribers::init_for_testing();
    let (
        object_ids,
        gas_object_ids,
        authority_state,
        epoch_store,
        transfer_transaction,
        metrics,
        _server,
        client,
        senders,
        multisig_pk,
    ) = setup_zklogin_network(|_| {}).await;
    let socket_addr = make_socket_addr();

    // case 1: a valid zklogin txn verifies ok, cache misses bc its a fresh zklogin inputs.
    let res = client
        .handle_transaction(transfer_transaction, Some(socket_addr))
        .await;
    assert!(res.is_ok());

    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_misses
            .get(),
        1
    );

    let (skp, _eph_pk, zklogin) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    let ephemeral_key = match skp {
        SuiKeyPair::Ed25519(kp) => kp,
        _ => panic!(),
    };
    let sender = senders[0];
    let recipient = dbg_addr(2);

    // case 2: use a different ephemeral key for a valid ephemeral pk + sig, but pk does
    // not match the zklogin inputs, cache misses and txn fails.
    let mut txn = init_zklogin_transfer(
        &authority_state,
        object_ids[2],
        gas_object_ids[2],
        recipient,
        sender,
        |_| {},
        ephemeral_key,
        zklogin,
    )
    .await;

    let intent_message = txn.clone().data().intent_message().clone();
    match &mut txn.data_mut_for_testing().tx_signatures_mut_for_testing()[0] {
        GenericSignature::ZkLoginAuthenticator(zklogin) => {
            let (_unknown_address, unknown_key): (_, AccountKeyPair) = get_key_pair();
            // replace the ephemeral signature with a bogus one
            *zklogin.user_signature_mut_for_testing() =
                Signature::new_secure(&intent_message, &unknown_key);
        }
        _ => panic!(),
    }

    assert!(matches!(
        client
            .handle_transaction(txn.clone(), Some(socket_addr))
            .await
            .unwrap_err(),
        SuiError::InvalidSignature { .. }
    ));
    assert_eq!(metrics.signature_errors.get(), 1);

    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_misses
            .get(),
        2
    );
    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_hits
            .get(),
        0
    );

    // case 3: use the same proof and same ephemeral key for a different txn
    // cache hits, also txn verifies.
    let txn3 = init_zklogin_transfer(
        &authority_state,
        object_ids[3],
        gas_object_ids[3],
        recipient,
        sender,
        |_| {},
        ephemeral_key,
        zklogin,
    )
    .await;

    assert!(client
        .handle_transaction(txn3, Some(socket_addr))
        .await
        .is_ok());

    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_hits
            .get(),
        1
    );

    // case 4: create a multisig txn where the zklogin signature inside is already cached
    // from the first call for the single zklogin txn. cache hits and txn verifies.
    let sender_2 = senders[1];
    let multisig_txn = sign_with_zklogin_inside_multisig(
        &authority_state,
        object_ids[11],
        gas_object_ids[11],
        recipient,
        sender_2,
        |_| {},
        ephemeral_key,
        zklogin,
        2,
        multisig_pk.clone(),
    )
    .await;
    assert!(client
        .handle_transaction(multisig_txn, Some(socket_addr))
        .await
        .is_ok());

    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_hits
            .get(),
        2
    );
    // case 5: use the same proof and modify ephemeral sig bytes but keep the ephemeral pk and flag as same,
    // it fails earlier at the ephemeral sig verify check, txn fails, did not miss or hit cache.
    let intent_message = txn.data().intent_message().clone();
    match &mut txn.data_mut_for_testing().tx_signatures_mut_for_testing()[0] {
        GenericSignature::ZkLoginAuthenticator(zklogin) => {
            let (_unknown_address, unknown_key): (_, AccountKeyPair) = get_key_pair();
            let correct_sig = Signature::new_secure(&intent_message, ephemeral_key);
            let unknown_sig = Signature::new_secure(&intent_message, &unknown_key);

            // create a mutated sig with the correct flag and pk bytes, but wrong signature bytes
            let mut mutated_bytes = vec![correct_sig.scheme().flag()];
            mutated_bytes.extend_from_slice(unknown_sig.signature_bytes());
            mutated_bytes.extend_from_slice(correct_sig.public_key_bytes());
            let mutated_sig = Signature::from_bytes(&mutated_bytes).unwrap();
            *zklogin.user_signature_mut_for_testing() = mutated_sig;
        }
        _ => panic!(),
    }

    assert!(matches!(
        client
            .handle_transaction(txn.clone(), Some(socket_addr))
            .await
            .unwrap_err(),
        SuiError::InvalidSignature { .. }
    ));
    assert_eq!(metrics.signature_errors.get(), 2);

    // cache hits unchanged
    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_hits
            .get(),
        2
    );

    // cache misses unchanged
    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_misses
            .get(),
        2
    );

    // case 6: use the same proof and modify max_epoch, cache misses and txn fails.
    let mut transfer_transaction3 = init_zklogin_transfer(
        &authority_state,
        object_ids[4],
        gas_object_ids[4],
        recipient,
        sender,
        |_| {},
        ephemeral_key,
        zklogin,
    )
    .await;
    match &mut transfer_transaction3
        .data_mut_for_testing()
        .tx_signatures_mut_for_testing()[0]
    {
        GenericSignature::ZkLoginAuthenticator(zklogin) => {
            *zklogin.max_epoch_mut_for_testing() += 1; // modify max epoch
        }
        _ => panic!(),
    }
    assert!(matches!(
        client
            .handle_transaction(transfer_transaction3.clone(), Some(socket_addr))
            .await
            .unwrap_err(),
        SuiError::InvalidSignature { .. }
    ));
    assert_eq!(metrics.signature_errors.get(), 3);

    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_misses
            .get(),
        3
    );

    // case 7: create a multisig txn with zklogin inside where the max epoch is changed,
    // cache misses and txn fails.
    let multisig_txn = sign_with_zklogin_inside_multisig(
        &authority_state,
        object_ids[11],
        gas_object_ids[11],
        recipient,
        sender_2,
        |_| {},
        ephemeral_key,
        zklogin,
        3, // modified from 2 to 3
        multisig_pk.clone(),
    )
    .await;
    assert!(matches!(
        client
            .handle_transaction(multisig_txn.clone(), Some(socket_addr))
            .await
            .unwrap_err(),
        SuiError::InvalidSignature { .. }
    ));

    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_misses
            .get(),
        4
    );

    // case 8: use the same proof and modify zklogin_inputs.address_seed, cache misses and txn fails.
    // use test_vectors[1] but with modified address_seed to create a bad zklogin inputs, and derive sender.
    let zklogin_json_string =
        &get_one_zklogin_inputs("../sui-types/src/unit_tests/zklogin_test_vectors.json");
    let bad_zklogin_inputs = ZkLoginInputs::from_json(zklogin_json_string, "111").unwrap();
    let sender = SuiAddress::try_from_unpadded(&bad_zklogin_inputs).unwrap();

    let mut txn4 = init_zklogin_transfer(
        &authority_state,
        object_ids[5],
        gas_object_ids[5],
        recipient,
        sender,
        |_| {},
        ephemeral_key,
        zklogin,
    )
    .await;
    match &mut txn4.data_mut_for_testing().tx_signatures_mut_for_testing()[0] {
        GenericSignature::ZkLoginAuthenticator(zklogin) => {
            *zklogin.zk_login_inputs_mut_for_testing() = bad_zklogin_inputs;
        }
        _ => panic!(),
    }

    assert!(matches!(
        client
            .handle_transaction(txn4.clone(), Some(socket_addr))
            .await
            .unwrap_err(),
        SuiError::InvalidSignature { .. }
    ));
    assert_eq!(metrics.signature_errors.get(), 5);

    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_misses
            .get(),
        5
    );
}

async fn do_zklogin_transaction_test(
    expected_sig_errors: u64,
    pre_sign_mutations: impl FnOnce(&mut TransactionData),
    post_sign_mutations: impl FnOnce(&mut Transaction),
) {
    let (
        object_ids,
        _gas_object_id,
        authority_state,
        epoch_store,
        mut transfer_transaction,
        metrics,
        _server,
        client,
        _senders,
        _multisig_pk,
    ) = setup_zklogin_network(pre_sign_mutations).await;

    post_sign_mutations(&mut transfer_transaction);

    assert!(client
        .handle_transaction(transfer_transaction, Some(make_socket_addr()))
        .await
        .is_err());

    assert_eq!(
        epoch_store
            .signature_verifier
            .metrics
            .zklogin_inputs_cache_misses
            .get(),
        1
    );

    assert_eq!(metrics.signature_errors.get(), expected_sig_errors);

    check_locks(authority_state, object_ids).await;
}

async fn check_locks(authority_state: Arc<AuthorityState>, object_ids: Vec<ObjectID>) {
    for object_id in object_ids {
        let object = authority_state.get_object(&object_id).await.unwrap();
        assert!(authority_state
            .get_transaction_lock(
                &object.compute_object_reference(),
                &authority_state.epoch_store_for_testing()
            )
            .await
            .unwrap()
            .is_none());
    }
}

async fn setup_zklogin_network(
    pre_sign_mutations: impl FnOnce(&mut TransactionData),
) -> (
    Vec<ObjectID>, // objects
    Vec<ObjectID>, // gas objects
    Arc<AuthorityState>,
    Guard<Arc<AuthorityPerEpochStore>>,
    sui_types::message_envelope::Envelope<SenderSignedData, sui_types::crypto::EmptySignInfo>,
    Arc<crate::authority_server::ValidatorServiceMetrics>,
    AuthorityServerHandle,
    NetworkAuthorityClient,
    Vec<SuiAddress>,
    MultiSigPublicKey,
) {
    let (skp, _eph_pk, zklogin) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    let ephemeral_key = match skp {
        SuiKeyPair::Ed25519(kp) => kp,
        _ => panic!(),
    };

    // a single zklogin address.
    let sender = SuiAddress::try_from_unpadded(zklogin).unwrap();

    // a 1-out-2 multisig address.
    let zklogin_pk = PublicKey::ZkLogin(
        ZkLoginPublicIdentifier::new(zklogin.get_iss(), zklogin.get_address_seed()).unwrap(),
    );
    let regular_pk = skp.public();
    let multisig_pk = MultiSigPublicKey::new(vec![zklogin_pk, regular_pk], vec![1, 1], 1).unwrap();
    let sender_2 = SuiAddress::from(&multisig_pk);

    let recipient = dbg_addr(2);
    let objects: Vec<_> = (0..20)
        .map(|i| match i < 10 {
            true => (sender, ObjectID::random()),
            false => (sender_2, ObjectID::random()),
        })
        .collect();
    let gas_objects: Vec<_> = (0..20)
        .map(|i| match i < 10 {
            true => (sender, ObjectID::random()),
            false => (sender_2, ObjectID::random()),
        })
        .collect();
    let object_ids: Vec<_> = objects.iter().map(|(_, id)| *id).collect();
    let gas_object_ids: Vec<_> = gas_objects.iter().map(|(_, id)| *id).collect();

    let authority_state =
        init_state_with_ids(objects.into_iter().chain(gas_objects).collect::<Vec<_>>()).await;

    let object_id = object_ids[0];
    let gas_object_id = gas_object_ids[0];
    let jwks = parse_jwks(DEFAULT_JWK_BYTES, &OIDCProvider::Twitch).unwrap();
    let epoch_store = authority_state.epoch_store_for_testing();
    epoch_store.update_authenticator_state(&AuthenticatorStateUpdate {
        epoch: 0,
        round: 0,
        new_active_jwks: jwks
            .into_iter()
            .map(|(jwk_id, jwk)| ActiveJwk {
                jwk_id,
                jwk,
                epoch: 0,
            })
            .collect(),
        authenticator_obj_initial_shared_version: 1.into(),
    });

    let transfer_transaction = init_zklogin_transfer(
        &authority_state,
        object_id,
        gas_object_id,
        recipient,
        sender,
        pre_sign_mutations,
        ephemeral_key,
        zklogin,
    )
    .await;

    let server = AuthorityServer::new_for_test(authority_state.clone());
    let metrics = server.metrics.clone();

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Some(
            authority_state
                .config
                .network_key_pair()
                .public()
                .to_owned(),
        ),
    )
    .await
    .unwrap();
    (
        object_ids,
        gas_object_ids,
        authority_state,
        epoch_store,
        transfer_transaction,
        metrics,
        server_handle,
        client,
        vec![sender, sender_2],
        multisig_pk,
    )
}

async fn init_zklogin_transfer(
    authority_state: &Arc<AuthorityState>,
    object_id: ObjectID,
    gas_object_id: ObjectID,
    recipient: SuiAddress,
    sender: SuiAddress,
    pre_sign_mutations: impl FnOnce(&mut TransactionData),
    ephemeral_key: &Ed25519KeyPair,
    zklogin: &ZkLoginInputs,
) -> sui_types::message_envelope::Envelope<SenderSignedData, sui_types::crypto::EmptySignInfo> {
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let object = authority_state.get_object(&object_id).await.unwrap();
    let gas_object = authority_state.get_object(&gas_object_id).await.unwrap();
    let object_ref = object.compute_object_reference();
    let gas_object_ref = gas_object.compute_object_reference();
    let gas_budget = rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER;
    let mut data = TransactionData::new_transfer(
        recipient,
        object_ref,
        sender,
        gas_object_ref,
        gas_budget,
        rgp,
    );
    pre_sign_mutations(&mut data);
    let mut tx = to_sender_signed_transaction(data, ephemeral_key);
    let GenericSignature::Signature(signature) =
        tx.data_mut_for_testing().tx_signatures_mut_for_testing()[0].clone()
    else {
        panic!();
    };
    let authenticator = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
        zklogin.clone(),
        2,
        signature,
    ));
    tx.data_mut_for_testing().tx_signatures_mut_for_testing()[0] = authenticator;
    tx
}

async fn sign_with_zklogin_inside_multisig(
    authority_state: &Arc<AuthorityState>,
    object_id: ObjectID,
    gas_object_id: ObjectID,
    recipient: SuiAddress,
    sender: SuiAddress,
    pre_sign_mutations: impl FnOnce(&mut TransactionData),
    ephemeral_key: &Ed25519KeyPair,
    zklogin: &ZkLoginInputs,
    max_epoch: u64,
    multisig_pk: MultiSigPublicKey,
) -> sui_types::message_envelope::Envelope<SenderSignedData, sui_types::crypto::EmptySignInfo> {
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let object = authority_state.get_object(&object_id).await.unwrap();
    let gas_object = authority_state.get_object(&gas_object_id).await.unwrap();
    let object_ref = object.compute_object_reference();
    let gas_object_ref = gas_object.compute_object_reference();
    let gas_budget = rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER;
    let mut data = TransactionData::new_transfer(
        recipient,
        object_ref,
        sender,
        gas_object_ref,
        gas_budget,
        rgp,
    );
    pre_sign_mutations(&mut data);
    let mut tx = to_sender_signed_transaction(data, ephemeral_key);
    let GenericSignature::Signature(signature) =
        tx.data_mut_for_testing().tx_signatures_mut_for_testing()[0].clone()
    else {
        panic!();
    };
    let multisig = MultiSig::combine(
        vec![GenericSignature::ZkLoginAuthenticator(
            ZkLoginAuthenticator::new(zklogin.clone(), max_epoch, signature),
        )],
        multisig_pk,
    )
    .unwrap();
    tx.data_mut_for_testing().tx_signatures_mut_for_testing()[0] =
        GenericSignature::MultiSig(multisig);
    tx
}

fn make_socket_addr() -> std::net::SocketAddr {
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0)
}

#[tokio::test]
async fn zklogin_txn_fail_if_missing_jwk() {
    telemetry_subscribers::init_for_testing();

    // Initialize an authorty state with some objects under a zklogin address.
    let (skp, _eph_pk, zklogin) =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1];
    let ephemeral_key = match skp {
        SuiKeyPair::Ed25519(kp) => kp,
        _ => panic!(),
    };
    let sender = SuiAddress::try_from_unpadded(zklogin).unwrap();
    let recipient = dbg_addr(2);
    let objects: Vec<_> = (0..10).map(|_| (sender, ObjectID::random())).collect();
    let gas_objects: Vec<_> = (0..10).map(|_| (sender, ObjectID::random())).collect();
    let object_ids: Vec<_> = objects.iter().map(|(_, id)| *id).collect();
    let gas_object_ids: Vec<_> = gas_objects.iter().map(|(_, id)| *id).collect();
    let authority_state =
        init_state_with_ids(objects.into_iter().chain(gas_objects).collect::<Vec<_>>()).await;

    // Initialize an authenticator state with a Google JWK.
    let jwks = parse_jwks(DEFAULT_JWK_BYTES, &OIDCProvider::Google).unwrap();
    let epoch_store = authority_state.epoch_store_for_testing();
    epoch_store.update_authenticator_state(&AuthenticatorStateUpdate {
        epoch: 0,
        round: 0,
        new_active_jwks: jwks
            .into_iter()
            .map(|(jwk_id, jwk)| ActiveJwk {
                jwk_id,
                jwk,
                epoch: 0,
            })
            .collect(),
        authenticator_obj_initial_shared_version: 1.into(),
    });

    // Case 1: Submit a transaction with zklogin signature derived from a Twitch JWT should fail.
    let txn1 = init_zklogin_transfer(
        &authority_state,
        object_ids[2],
        gas_object_ids[2],
        recipient,
        sender,
        |_| {},
        ephemeral_key,
        zklogin,
    )
    .await;
    execute_transaction_assert_err(authority_state.clone(), txn1.clone(), object_ids.clone()).await;

    // Initialize an authenticator state with Twitch's kid as "nosuckkey".
    pub const BAD_JWK_BYTES: &[u8] = r#"{"keys":[{"alg":"RS256","e":"AQAB","kid":"nosuchkey","kty":"RSA","n":"6lq9MQ-q6hcxr7kOUp-tHlHtdcDsVLwVIw13iXUCvuDOeCi0VSuxCCUY6UmMjy53dX00ih2E4Y4UvlrmmurK0eG26b-HMNNAvCGsVXHU3RcRhVoHDaOwHwU72j7bpHn9XbP3Q3jebX6KIfNbei2MiR0Wyb8RZHE-aZhRYO8_-k9G2GycTpvc-2GBsP8VHLUKKfAs2B6sW3q3ymU6M0L-cFXkZ9fHkn9ejs-sqZPhMJxtBPBxoUIUQFTgv4VXTSv914f_YkNw-EjuwbgwXMvpyr06EyfImxHoxsZkFYB-qBYHtaMxTnFsZBr6fn8Ha2JqT1hoP7Z5r5wxDu3GQhKkHw","use":"sig"}]}"#.as_bytes();
    let jwks = parse_jwks(BAD_JWK_BYTES, &OIDCProvider::Twitch).unwrap();
    epoch_store.update_authenticator_state(&AuthenticatorStateUpdate {
        epoch: 0,
        round: 0,
        new_active_jwks: jwks
            .into_iter()
            .map(|(jwk_id, jwk)| ActiveJwk {
                jwk_id,
                jwk,
                epoch: 0,
            })
            .collect(),
        authenticator_obj_initial_shared_version: 1.into(),
    });

    // Case 2: Submit a transaction with zklogin signature derived from a Twitch JWT with kid "1" should fail.
    execute_transaction_assert_err(authority_state, txn1, object_ids).await;
}

#[tokio::test]
async fn zk_multisig_test() {
    telemetry_subscribers::init_for_testing();

    // User generate a multisig account with no zklogin signer.
    let keys = sui_types::utils::keys();
    let pk1 = keys[0].public();
    let pk2 = keys[1].public();
    let pk3 = keys[2].public();
    let multisig_pk = MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let victim_addr = SuiAddress::from(&multisig_pk);

    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(victim_addr, object_id), (victim_addr, gas_object_id)]).await;

    let jwks = parse_jwks(DEFAULT_JWK_BYTES, &OIDCProvider::Twitch).unwrap();
    let epoch_store = authority_state.epoch_store_for_testing();
    epoch_store.update_authenticator_state(&AuthenticatorStateUpdate {
        epoch: 0,
        round: 0,
        new_active_jwks: jwks
            .into_iter()
            .map(|(jwk_id, jwk)| ActiveJwk {
                jwk_id,
                jwk,
                epoch: 0,
            })
            .collect(),
        authenticator_obj_initial_shared_version: 1.into(),
    });

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let object = authority_state.get_object(&object_id).await.unwrap();
    let gas_object = authority_state.get_object(&gas_object_id).await.unwrap();

    let data = TransactionData::new_transfer(
        recipient,
        object.compute_object_reference(),
        victim_addr,
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    // Step 1. construct 2 zklogin signatures
    let test_vectors =
        &load_test_vectors("../sui-types/src/unit_tests/zklogin_test_vectors.json")[1..];
    let mut zklogin_sigs = vec![];
    for (kp, _pk_zklogin, inputs) in test_vectors {
        let intent_message = IntentMessage::new(Intent::sui_transaction(), data.clone());
        let eph_sig = Signature::new_secure(&intent_message, kp);
        let zklogin_sig = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
            inputs.clone(),
            2,
            eph_sig,
        ));
        zklogin_sigs.push(zklogin_sig);
    }

    // Step 2. Construct the fake multisig with the zklogin signatures.
    let multisig = MultiSig::insecure_new(
        vec![
            zklogin_sigs[0].clone().to_compressed().unwrap(),
            zklogin_sigs[1].clone().to_compressed().unwrap(),
        ], // zklogin sigs
        3,
        multisig_pk,
    );
    let generic_sig = GenericSignature::MultiSig(multisig);
    let transfer_transaction = Transaction::from_generic_sig_data(data, vec![generic_sig]);

    execute_transaction_assert_err(authority_state, transfer_transaction, vec![object_id]).await;
}

async fn execute_transaction_assert_err(
    authority_state: Arc<AuthorityState>,
    txn: Transaction,
    object_ids: Vec<ObjectID>,
) {
    let server = AuthorityServer::new_for_test(authority_state.clone());

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Some(
            authority_state
                .config
                .network_key_pair()
                .public()
                .to_owned(),
        ),
    )
    .await
    .unwrap();
    let err = client
        .handle_transaction(txn.clone(), Some(make_socket_addr()))
        .await;

    assert!(dbg!(err).is_err());

    check_locks(authority_state.clone(), object_ids).await;
}

#[tokio::test]
async fn test_oversized_txn() {
    telemetry_subscribers::init_for_testing();
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;
    let max_txn_size = authority_state
        .epoch_store_for_testing()
        .protocol_config()
        .max_tx_size_bytes() as usize;
    let object = authority_state.get_object(&object_id).await.unwrap();
    let obj_ref = object.compute_object_reference();

    // Construct an oversized txn.
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        // Put a lot of commands in the txn so it's large.
        for _ in 0..(1024 * 16) {
            builder.transfer_object(recipient, obj_ref).unwrap();
        }
        builder.finish()
    };

    let txn_data = TransactionData::new_programmable(sender, vec![obj_ref], pt, 0, 0);

    let txn = to_sender_signed_transaction(txn_data, &sender_key);
    let tx_size = bcs::serialized_size(&txn).unwrap();

    // Making sure the txn is larger than the max txn size.
    assert!(tx_size > max_txn_size);

    let server = AuthorityServer::new_for_test(authority_state.clone());

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Some(
            authority_state
                .config
                .network_key_pair()
                .public()
                .to_owned(),
        ),
    )
    .await
    .unwrap();

    let res = client
        .handle_transaction(txn, Some(make_socket_addr()))
        .await;
    // The txn should be rejected due to its size.
    assert!(res
        .err()
        .unwrap()
        .to_string()
        .contains("serialized transaction size exceeded maximum"));
}

#[tokio::test]
async fn test_very_large_certificate() {
    telemetry_subscribers::init_for_testing();
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let object = authority_state.get_object(&object_id).await.unwrap();
    let gas_object = authority_state.get_object(&gas_object_id).await.unwrap();

    let transfer_transaction = init_transfer_transaction(
        |_| {},
        sender,
        &sender_key,
        recipient,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    let server = AuthorityServer::new_for_test(authority_state.clone());

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Some(
            authority_state
                .config
                .network_key_pair()
                .public()
                .to_owned(),
        ),
    )
    .await
    .unwrap();
    let socket_addr = make_socket_addr();

    let auth_sig = client
        .handle_transaction(transfer_transaction.clone(), Some(socket_addr))
        .await
        .unwrap()
        .status
        .into_signed_for_testing();

    let signatures: BTreeMap<_, _> = vec![auth_sig]
        .into_iter()
        .map(|a| (a.authority, a.signature))
        .collect();

    // Insert a lot into the bitmap so the cert is very large, while the txn inside is reasonably sized.
    let mut signers_map = roaring::bitmap::RoaringBitmap::new();
    signers_map.insert_range(0..52108864);
    let sigs: Vec<AuthoritySignature> = signatures.into_values().collect();

    let quorum_signature = sui_types::crypto::AuthorityQuorumSignInfo {
        epoch: 0,
        signature: sui_types::crypto::AggregateAuthoritySignature::aggregate(&sigs)
            .map_err(|e| SuiError::InvalidSignature {
                error: e.to_string(),
            })
            .expect("Validator returned invalid signature"),
        signers_map,
    };
    let cert = sui_types::message_envelope::Envelope::new_from_data_and_sig(
        transfer_transaction.into_data(),
        quorum_signature,
    );

    let res = client.handle_certificate_v2(cert, Some(socket_addr)).await;
    assert!(res.is_err());
    let err = res.err().unwrap();
    // The resulting error should be a RpcError with a message length too large.
    assert!(
        matches!(err, SuiError::RpcError(..))
            && err.to_string().contains("message length too large")
    );
}

#[tokio::test]
async fn test_handle_certificate_errors() {
    telemetry_subscribers::init_for_testing();
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let object = authority_state.get_object(&object_id).await.unwrap();
    let gas_object = authority_state.get_object(&gas_object_id).await.unwrap();

    let transfer_transaction = init_transfer_transaction(
        |_| {},
        sender,
        &sender_key,
        recipient,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    let server = AuthorityServer::new_for_test(authority_state.clone());

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Some(
            authority_state
                .config
                .network_key_pair()
                .public()
                .to_owned(),
        ),
    )
    .await
    .unwrap();

    // Test handle certificate from the wrong epoch
    let epoch_store = authority_state.epoch_store_for_testing();
    let next_epoch = epoch_store.epoch() + 1;
    let signed_transaction = VerifiedSignedTransaction::new(
        next_epoch,
        VerifiedTransaction::new_unchecked(transfer_transaction.clone()),
        authority_state.name,
        &*authority_state.secret,
    );

    let mut committee_1 = epoch_store.committee().deref().clone();
    committee_1.epoch = next_epoch;
    let ct = CertifiedTransaction::new(
        transfer_transaction.data().clone(),
        vec![signed_transaction.auth_sig().clone()],
        &committee_1,
    )
    .unwrap();
    let socket_addr = make_socket_addr();

    let err = client
        .handle_certificate_v2(ct.clone(), Some(socket_addr))
        .await
        .unwrap_err();
    assert_matches!(
        err,
        SuiError::WrongEpoch {
            expected_epoch: 0,
            actual_epoch: 1
        }
    );

    // Test handle certificate with invalid user input
    let signed_transaction = VerifiedSignedTransaction::new(
        epoch_store.epoch(),
        VerifiedTransaction::new_unchecked(transfer_transaction.clone().clone()),
        authority_state.name,
        &*authority_state.secret,
    );

    let committee = epoch_store.committee().deref().clone();

    let tx = VerifiedTransaction::new_consensus_commit_prologue(0, 0, 42);
    let ct = CertifiedTransaction::new(
        tx.data().clone(),
        vec![signed_transaction.auth_sig().clone()],
        &committee,
    )
    .unwrap();

    let err = client
        .handle_certificate_v2(ct.clone(), Some(socket_addr))
        .await
        .unwrap_err();

    assert_matches!(
        err,
        SuiError::UserInputError {
            error: UserInputError::Unsupported(message)
        } if message == "SenderSignedData must not contain system transaction"
    );

    let mut invalid_sig_count_tx = transfer_transaction.clone();
    let data = invalid_sig_count_tx.data_mut_for_testing();
    data.tx_signatures_mut_for_testing().clear();
    let ct = CertifiedTransaction::new(
        data.clone(),
        vec![signed_transaction.auth_sig().clone()],
        &committee,
    )
    .unwrap();
    let err = client
        .handle_certificate_v2(ct.clone(), Some(socket_addr))
        .await
        .unwrap_err();

    assert_matches!(
        err,
        SuiError::SignerSignatureNumberMismatch {
            expected: 1,
            actual: 0
        }
    );

    let mut absent_sig_tx = transfer_transaction.clone();
    let (_unknown_address, unknown_key): (_, AccountKeyPair) = get_key_pair();
    let data = absent_sig_tx.data_mut_for_testing();
    *data.tx_signatures_mut_for_testing() =
        vec![Signature::new_secure(data.intent_message(), &unknown_key).into()];
    let ct = CertifiedTransaction::new(
        data.clone(),
        vec![signed_transaction.auth_sig().clone()],
        &committee,
    )
    .unwrap();

    let err = client
        .handle_certificate_v2(ct.clone(), Some(socket_addr))
        .await
        .unwrap_err();

    assert_matches!(err, SuiError::SignerSignatureAbsent { .. });
}

#[sim_test]
async fn test_handle_soft_bundle_certificates() {
    telemetry_subscribers::init_for_testing();

    let mut protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    protocol_config.set_enable_soft_bundle_for_testing(true);
    protocol_config.set_max_soft_bundle_size_for_testing(10);

    let authority = TestAuthorityBuilder::new()
        .with_reference_gas_price(1000)
        .with_protocol_config(protocol_config)
        .build()
        .await;

    let mut senders = Vec::new();
    let mut gas_object_ids = Vec::new();
    for _i in 0..4 {
        let (address, keypair): (_, AccountKeyPair) = get_key_pair();
        let gas_object_id = ObjectID::random();

        let obj = Object::with_id_owner_for_testing(gas_object_id, address);
        authority.insert_genesis_object(obj).await;

        senders.push((address, keypair));
        gas_object_ids.push(gas_object_id);
    }

    let (authority, package) = publish_object_basics(authority).await;

    let shared_object = {
        let effects = call_move_(
            &authority,
            None,
            &gas_object_ids[0],
            &senders[0].0,
            &senders[0].1,
            &package.0,
            "object_basics",
            "share",
            vec![],
            vec![],
            true,
        )
        .await
        .unwrap();
        effects.status().unwrap();
        let shared_object_id = effects.created()[0].0 .0;
        authority.get_object(&shared_object_id).await.unwrap()
    };
    let initial_shared_version = shared_object.version();

    // Create a server with mocked consensus.
    // This ensures transactions submitted to consensus will get processed.
    let adapter = make_consensus_adapter_for_test(
        authority.clone(),
        HashSet::new(),
        true,
        vec![with_block_status(BlockStatus::Sequenced(BlockRef::MIN))],
    );
    let server = AuthorityServer::new_for_test_with_consensus_adapter(authority.clone(), adapter);
    let _metrics = server.metrics.clone();
    let server_handle = server.spawn_for_test().await.unwrap();
    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Some(authority.config.network_key_pair().public().to_owned()),
    )
    .await
    .unwrap();

    let signed_tx_into_certificate = |transaction: Transaction| async {
        let epoch_store = authority.load_epoch_store_one_call_per_task();
        let committee = authority.clone_committee_for_testing();
        let mut sigs = vec![];

        let transaction = epoch_store.verify_transaction(transaction).unwrap();
        let response = authority
            .handle_transaction(&epoch_store, transaction.clone())
            .await
            .unwrap();
        let vote = response.status.into_signed_for_testing();
        sigs.push(vote.clone());
        if let Ok(cert) =
            CertifiedTransaction::new(transaction.clone().into_message(), sigs.clone(), &committee)
        {
            return cert
                .try_into_verified_for_testing(&committee, &Default::default())
                .unwrap();
        }
        panic!("Failed to create certificate");
    };

    let rgp = authority.reference_gas_price_for_testing().unwrap();
    let mut certificates: Vec<CertifiedTransaction> = Vec::new();
    for i in 0..4 {
        let cert = {
            let gas_object_ref = authority
                .get_object(&gas_object_ids[i])
                .await
                .unwrap()
                .compute_object_reference();
            let data = TransactionData::new_move_call(
                senders[i].0,
                package.0,
                ident_str!("object_basics").to_owned(),
                ident_str!("set_value").to_owned(),
                /* type_args */ vec![],
                gas_object_ref,
                /* args */
                vec![
                    CallArg::Object(ObjectArg::SharedObject {
                        id: shared_object.id(),
                        initial_shared_version,
                        mutable: true,
                    }),
                    CallArg::Pure((i as u64).to_le_bytes().to_vec()),
                ],
                TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
                rgp,
            )
            .unwrap();
            let signed = to_sender_signed_transaction(data, &senders[i].1);
            signed_tx_into_certificate(signed).await
        };
        certificates.push(cert.into());
    }
    let responses = client
        .handle_soft_bundle_certificates_v3(
            HandleSoftBundleCertificatesRequestV3 {
                certificates,
                wait_for_effects: true,
                include_events: false,
                include_auxiliary_data: false,
                include_input_objects: true,
                include_output_objects: true,
            },
            None,
        )
        .await
        .unwrap()
        .responses;

    // Verify if transactions have been executed in the correct order.
    // This is done by checking if each tx's input object version matches the previous tx's output object version.
    assert_eq!(responses.len(), 4);
    let mut expected_object_version = initial_shared_version;
    for response in responses {
        let input_objects = response.input_objects.unwrap();
        assert!(input_objects
            .iter()
            .any(|obj| obj.id() == shared_object.id() && obj.version() == expected_object_version));

        let output_objects = response.output_objects.unwrap();
        let output_object = output_objects
            .iter()
            .find(|obj| obj.id() == shared_object.id())
            .unwrap();
        expected_object_version = output_object.version();
    }
}

#[tokio::test]
async fn test_handle_soft_bundle_certificates_errors() {
    let mut senders = Vec::new();
    let mut gas_objects = Vec::new();
    let mut owned_objects = Vec::new();
    for _i in 0..15 {
        let (sender, keypair): (_, AccountKeyPair) = get_key_pair();
        let mut objects = create_gas_objects(2, sender);
        senders.push((sender, keypair));
        gas_objects.push(objects.remove(0));
        owned_objects.push(objects.remove(0));
    }

    let mut protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    protocol_config.set_enable_soft_bundle_for_testing(true);
    protocol_config.set_max_soft_bundle_size_for_testing(3);
    protocol_config.set_consensus_max_transactions_in_block_bytes_for_testing(10_000);
    let authority = TestAuthorityBuilder::new()
        .with_reference_gas_price(1000)
        .with_protocol_config(protocol_config)
        .build()
        .await;

    authority.insert_genesis_objects(&gas_objects).await;
    authority.insert_genesis_objects(&owned_objects).await;

    let (authority, package) = publish_object_basics(authority).await;

    let shared_object = {
        let effects = call_move_(
            &authority,
            None,
            &gas_objects[3].id(),
            &senders[3].0,
            &senders[3].1,
            &package.0,
            "object_basics",
            "share",
            vec![],
            vec![],
            true,
        )
        .await
        .unwrap();
        effects.status().unwrap();
        let shared_object_id = effects.created()[0].0 .0;
        authority.get_object(&shared_object_id).await.unwrap()
    };
    let initial_shared_version = shared_object.version();

    // Create a single validator cluster.
    let server = AuthorityServer::new_for_test(authority);
    let authority = server.state.clone();
    let _metrics = server.metrics.clone();
    let server_handle = server.spawn_for_test().await.unwrap();
    let client = NetworkAuthorityClient::connect(
        server_handle.address(),
        Some(authority.config.network_key_pair().public().to_owned()),
    )
    .await
    .unwrap();

    let signed_tx_into_certificate = |transaction: Transaction| async {
        let epoch_store = authority.load_epoch_store_one_call_per_task();
        let committee = authority.clone_committee_for_testing();
        let mut sigs = vec![];

        let transaction = epoch_store.verify_transaction(transaction).unwrap();
        let response = authority
            .handle_transaction(&epoch_store, transaction.clone())
            .await
            .unwrap();
        let vote = response.status.into_signed_for_testing();
        sigs.push(vote.clone());
        if let Ok(cert) =
            CertifiedTransaction::new(transaction.clone().into_message(), sigs.clone(), &committee)
        {
            return cert
                .try_into_verified_for_testing(&committee, &Default::default())
                .unwrap();
        }
        panic!("Failed to create certificate");
    };

    let rgp = authority.reference_gas_price_for_testing().unwrap();

    // Case 0: submit an empty soft bundle.
    println!("Case 0: submit an empty soft bundle.");
    {
        let response = client
            .handle_soft_bundle_certificates_v3(
                HandleSoftBundleCertificatesRequestV3 {
                    certificates: vec![],
                    wait_for_effects: true,
                    include_events: false,
                    include_auxiliary_data: false,
                    include_input_objects: false,
                    include_output_objects: false,
                },
                None,
            )
            .await;
        assert!(response.is_err());
        assert_matches!(
            response.unwrap_err(),
            SuiError::NoCertificateProvidedError { .. }
        );
    }

    // Case 1: submit a soft bundle with more txs than the limit.
    // The bundle should be rejected.
    println!("Case 1: submit a soft bundle with more txs than the limit.");
    {
        let mut certificates: Vec<CertifiedTransaction> = vec![];
        for i in 0..5 {
            let owned_object_ref = authority
                .get_object(&owned_objects[i].id())
                .await
                .unwrap()
                .compute_object_reference();
            let gas_object_ref = authority
                .get_object(&gas_objects[i].id())
                .await
                .unwrap()
                .compute_object_reference();
            let data = TransactionData::new_transfer(
                senders[i + 1].0,
                owned_object_ref,
                senders[i].0,
                gas_object_ref,
                rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
                rgp,
            );
            let signed = to_sender_signed_transaction(data, &senders[i].1);
            certificates.push(signed_tx_into_certificate(signed).await.into());
        }
        let response = client
            .handle_soft_bundle_certificates_v3(
                HandleSoftBundleCertificatesRequestV3 {
                    certificates,
                    wait_for_effects: true,
                    include_events: false,
                    include_auxiliary_data: false,
                    include_input_objects: false,
                    include_output_objects: false,
                },
                None,
            )
            .await;
        assert!(response.is_err());
        assert_matches!(
            response.unwrap_err(),
            SuiError::UserInputError {
                error: UserInputError::TooManyTransactionsInSoftBundle { .. },
            }
        );
    }

    // Case 2: submit a soft bundle with tx containing no shared object.
    // The bundle should be rejected.
    println!("Case 2: submit a soft bundle with tx containing no shared object.");
    {
        let owned_object_ref = authority
            .get_object(&owned_objects[5].id())
            .await
            .unwrap()
            .compute_object_reference();
        let gas_object_ref = authority
            .get_object(&gas_objects[5].id())
            .await
            .unwrap()
            .compute_object_reference();
        let data = TransactionData::new_transfer(
            senders[6].0,
            owned_object_ref,
            senders[5].0,
            gas_object_ref,
            rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
            rgp,
        );
        let signed = to_sender_signed_transaction(data, &senders[5].1);
        let response = client
            .handle_soft_bundle_certificates_v3(
                HandleSoftBundleCertificatesRequestV3 {
                    certificates: vec![signed_tx_into_certificate(signed).await.into()],
                    wait_for_effects: true,
                    include_events: false,
                    include_auxiliary_data: false,
                    include_input_objects: false,
                    include_output_objects: false,
                },
                None,
            )
            .await;
        assert!(response.is_err());
        assert_matches!(
            response.unwrap_err(),
            SuiError::UserInputError {
                error: UserInputError::NoSharedObjectError { .. },
            }
        );
    }

    // Case 3: submit a soft bundle with txs of different gas prices.
    // The bundle should be rejected.
    println!("Case 3: submit a soft bundle with txs of different gas prices.");
    {
        let cert0 = {
            let gas_object_ref = authority
                .get_object(&gas_objects[6].id())
                .await
                .unwrap()
                .compute_object_reference();
            let data = TransactionData::new_move_call(
                senders[6].0,
                package.0,
                ident_str!("object_basics").to_owned(),
                ident_str!("set_value").to_owned(),
                /* type_args */ vec![],
                gas_object_ref,
                /* args */
                vec![
                    CallArg::Object(ObjectArg::SharedObject {
                        id: shared_object.id(),
                        initial_shared_version,
                        mutable: true,
                    }),
                    CallArg::Pure(11u64.to_le_bytes().to_vec()),
                ],
                TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
                rgp,
            )
            .unwrap();
            let signed = to_sender_signed_transaction(data, &senders[6].1);
            signed_tx_into_certificate(signed).await
        };
        let cert1 = {
            let gas_object_ref = authority
                .get_object(&gas_objects[7].id())
                .await
                .unwrap()
                .compute_object_reference();
            let data = TransactionData::new_move_call(
                senders[7].0,
                package.0,
                ident_str!("object_basics").to_owned(),
                ident_str!("set_value").to_owned(),
                /* type_args */ vec![],
                gas_object_ref,
                /* args */
                vec![
                    CallArg::Object(ObjectArg::SharedObject {
                        id: shared_object.id(),
                        initial_shared_version,
                        mutable: true,
                    }),
                    CallArg::Pure(12u64.to_le_bytes().to_vec()),
                ],
                TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
                rgp + 1,
            )
            .unwrap();
            let signed = to_sender_signed_transaction(data, &senders[7].1);
            signed_tx_into_certificate(signed).await
        };
        let response = client
            .handle_soft_bundle_certificates_v3(
                HandleSoftBundleCertificatesRequestV3 {
                    certificates: vec![cert0.into(), cert1.into()],
                    wait_for_effects: true,
                    include_events: false,
                    include_auxiliary_data: false,
                    include_input_objects: false,
                    include_output_objects: false,
                },
                None,
            )
            .await;
        assert!(response.is_err());
        assert_matches!(
            response.unwrap_err(),
            SuiError::UserInputError {
                error: UserInputError::GasPriceMismatchError { .. },
            }
        );
    }

    // Case 4: submit a soft bundle with txs whose consensus message has been processed.
    // The bundle should be rejected.
    println!("Case 4: submit a soft bundle with txs whose consensus message has been processed.");
    {
        let cert0 = {
            let gas_object_ref = authority
                .get_object(&gas_objects[8].id())
                .await
                .unwrap()
                .compute_object_reference();
            let data = TransactionData::new_move_call(
                senders[8].0,
                package.0,
                ident_str!("object_basics").to_owned(),
                ident_str!("set_value").to_owned(),
                /* type_args */ vec![],
                gas_object_ref,
                /* args */
                vec![
                    CallArg::Object(ObjectArg::SharedObject {
                        id: shared_object.id(),
                        initial_shared_version,
                        mutable: true,
                    }),
                    CallArg::Pure(11u64.to_le_bytes().to_vec()),
                ],
                TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
                rgp,
            )
            .unwrap();
            let signed = to_sender_signed_transaction(data, &senders[8].1);
            signed_tx_into_certificate(signed).await
        };
        let cert1 = {
            let gas_object_ref = authority
                .get_object(&gas_objects[9].id())
                .await
                .unwrap()
                .compute_object_reference();
            let data = TransactionData::new_move_call(
                senders[9].0,
                package.0,
                ident_str!("object_basics").to_owned(),
                ident_str!("set_value").to_owned(),
                /* type_args */ vec![],
                gas_object_ref,
                /* args */
                vec![
                    CallArg::Object(ObjectArg::SharedObject {
                        id: shared_object.id(),
                        initial_shared_version,
                        mutable: true,
                    }),
                    CallArg::Pure(12u64.to_le_bytes().to_vec()),
                ],
                TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
                rgp,
            )
            .unwrap();
            let signed = to_sender_signed_transaction(data, &senders[9].1);
            signed_tx_into_certificate(signed).await
        };
        send_batch_consensus_no_execution(&authority, &vec![cert0.clone(), cert1.clone()], true)
            .await;
        let response = client
            .handle_soft_bundle_certificates_v3(
                HandleSoftBundleCertificatesRequestV3 {
                    certificates: vec![cert0.into(), cert1.into()],
                    wait_for_effects: true,
                    include_events: false,
                    include_auxiliary_data: false,
                    include_input_objects: false,
                    include_output_objects: false,
                },
                None,
            )
            .await;
        assert!(response.is_err());
        assert_matches!(
            response.unwrap_err(),
            SuiError::UserInputError {
                error: UserInputError::CertificateAlreadyProcessed { .. },
            }
        );
    }

    // Case 5: submit a soft bundle with total tx size exceeding the block size limit.
    // The bundle should be rejected.
    println!("Case 5: submit a soft bundle with total tx size exceeding the block size limit.");
    {
        let mut certificates: Vec<CertifiedTransaction> = vec![];

        for i in 11..14 {
            let owned_object_ref = authority
                .get_object(&owned_objects[i].id())
                .await
                .unwrap()
                .compute_object_reference();
            let gas_object_ref = authority
                .get_object(&gas_objects[i].id())
                .await
                .unwrap()
                .compute_object_reference();
            let sender = &senders[i];
            let recipient = &senders[i + 1].0;

            // Construct an oversized txn.
            let pt = {
                let mut builder = ProgrammableTransactionBuilder::new();
                // Put a lot of commands in the txn so it's large.
                for _ in 0..1000 {
                    builder
                        .transfer_object(*recipient, owned_object_ref)
                        .unwrap();
                }
                builder.finish()
            };

            let data = TransactionData::new_programmable(
                sender.0,
                vec![gas_object_ref],
                pt,
                rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
                rgp,
            );

            let signed = to_sender_signed_transaction(data, &sender.1);
            certificates.push(signed_tx_into_certificate(signed).await.into());
        }

        let response = client
            .handle_soft_bundle_certificates_v3(
                HandleSoftBundleCertificatesRequestV3 {
                    certificates,
                    wait_for_effects: true,
                    include_events: false,
                    include_auxiliary_data: false,
                    include_input_objects: false,
                    include_output_objects: false,
                },
                None,
            )
            .await;
        assert!(response.is_err());
        assert_matches!(
            response.unwrap_err(),
            SuiError::UserInputError {
                error: UserInputError::SoftBundleTooLarge {
                    size: 25116,
                    limit: 5000
                },
            }
        );
    }
}

#[test]
fn sender_signed_data_serialized_intent() {
    let mut txn = SenderSignedData::new(
        TransactionData::new_transfer(
            SuiAddress::default(),
            random_object_ref(),
            SuiAddress::default(),
            random_object_ref(),
            0,
            0,
        ),
        vec![],
    );

    assert_eq!(txn.intent_message().intent, Intent::sui_transaction());

    // deser fails when intent is wrong
    let mut bytes = bcs::to_bytes(txn.inner()).unwrap();
    bytes[0] = 1; // set invalid intent
    let e = bcs::from_bytes::<SenderSignedTransaction>(&bytes).unwrap_err();
    assert!(e.to_string().contains("invalid Intent for Transaction"));

    // ser fails when intent is wrong
    txn.inner_mut().intent_message.intent.scope = IntentScope::TransactionEffects;
    let e = bcs::to_bytes(txn.inner()).unwrap_err();
    assert!(e.to_string().contains("invalid Intent for Transaction"));
}
