use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::crypto::Signature;
use sui_types::{signature::GenericSignature, transaction::Transaction, zk_login_authenticator::ZkLoginAuthenticator};

use shared_crypto::intent::Intent;
use shared_crypto::intent::IntentMessage;

use crate::mock_account::ZkLoginEphKeyPair;
use crate::mock_account::MockKeyPair;

pub fn build_and_sign(
    tx: TestTransactionBuilder,
    keypair: &MockKeyPair
) -> Transaction {
    match keypair {
        MockKeyPair::Ed25519(kp) => {
            tx.build_and_sign(kp)
        }
        MockKeyPair::ZkLogin(kp) => {
            build_and_sign_zklogin(tx, kp)
        }
    }
}

pub fn build_and_sign_zklogin(
    tx: TestTransactionBuilder,
    kp: &ZkLoginEphKeyPair,
) -> Transaction {
    let data = tx.build();

    let msg = IntentMessage::new(Intent::sui_transaction(), data.clone());
    let eph_sig = Signature::new_secure(&msg, &kp.private);

    let generic_sig = GenericSignature::ZkLoginAuthenticator(
        ZkLoginAuthenticator::new(
            kp.public.zkp_details.clone(),
            kp.public.max_epoch,
            eph_sig.clone(),
        )
    );

    Transaction::from_generic_sig_data(data, vec![generic_sig])
}
