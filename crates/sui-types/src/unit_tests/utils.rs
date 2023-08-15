// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::crypto::{Signer, SuiKeyPair};
use crate::multisig::{MultiSig, MultiSigPublicKey};
use crate::programmable_transaction_builder::ProgrammableTransactionBuilder;
use crate::transaction::{SenderSignedData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER};
use crate::SuiAddress;
use crate::{
    base_types::{dbg_addr, ExecutionDigests, ObjectID},
    committee::Committee,
    crypto::{
        get_key_pair, get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair,
        AuthorityPublicKeyBytes, DefaultHash, Signature, SignatureScheme,
    },
    gas::GasCostSummary,
    messages_checkpoint::{
        CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary, SignedCheckpointSummary,
    },
    object::Object,
    signature::GenericSignature,
    transaction::{Transaction, TransactionData},
    zk_login_authenticator::ZkLoginAuthenticator,
};
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::hash::HashFunction;
use fastcrypto::traits::KeyPair as KeypairTraits;
use rand::rngs::StdRng;
use rand::SeedableRng;
use shared_crypto::intent::{Intent, IntentMessage};
use std::collections::BTreeMap;

pub const TEST_CLIENT_ID: &str =
    "575519204237-msop9ep45u2uo98hapqmngv8d84qdc8k.apps.googleusercontent.com";

pub fn make_committee_key<R>(rand: &mut R) -> (Vec<AuthorityKeyPair>, Committee)
where
    R: rand::CryptoRng + rand::RngCore,
{
    make_committee_key_num(4, rand)
}

pub fn make_committee_key_num<R>(num: usize, rand: &mut R) -> (Vec<AuthorityKeyPair>, Committee)
where
    R: rand::CryptoRng + rand::RngCore,
{
    let mut authorities: BTreeMap<AuthorityPublicKeyBytes, u64> = BTreeMap::new();
    let mut keys = Vec::new();

    for _ in 0..num {
        let (_, inner_authority_key): (_, AuthorityKeyPair) = get_key_pair_from_rng(rand);
        authorities.insert(
            /* address */ AuthorityPublicKeyBytes::from(inner_authority_key.public()),
            /* voting right */ 1,
        );
        keys.push(inner_authority_key);
    }

    let committee = Committee::new_for_testing_with_normalized_voting_power(0, authorities);
    (keys, committee)
}

// Creates a fake sender-signed transaction for testing. This transaction will
// not actually work.
pub fn create_fake_transaction() -> Transaction {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let object = Object::immutable_with_id_for_testing(object_id);
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(recipient, None);
        builder.finish()
    };
    let data = TransactionData::new_programmable(
        sender,
        vec![object.compute_object_reference()],
        pt,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER, // gas price is 1
        1,
    );
    to_sender_signed_transaction(data, &sender_key)
}

fn make_transaction_data(sender: SuiAddress) -> TransactionData {
    let object = Object::immutable_with_id_for_testing(ObjectID::random_from_rng(
        &mut StdRng::from_seed([0; 32]),
    ));
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(dbg_addr(2), None);
        builder.finish()
    };
    TransactionData::new_programmable(
        sender,
        vec![object.compute_object_reference()],
        pt,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER, // gas price is 1
        1,
    )
}

/// Make a user signed transaction with the given sender and its keypair. This
/// is not verified or signed by authority.
pub fn make_transaction(sender: SuiAddress, kp: &SuiKeyPair, intent: Intent) -> Transaction {
    let data = make_transaction_data(sender);
    Transaction::from_data_and_signer(data, intent, vec![kp])
}

// This is used to sign transaction with signer using default Intent.
pub fn to_sender_signed_transaction(
    data: TransactionData,
    signer: &dyn Signer<Signature>,
) -> Transaction {
    to_sender_signed_transaction_with_multi_signers(data, vec![signer])
}

pub fn to_sender_signed_transaction_with_multi_signers(
    data: TransactionData,
    signers: Vec<&dyn Signer<Signature>>,
) -> Transaction {
    Transaction::from_data_and_signer(data, Intent::sui_transaction(), signers)
}

pub fn mock_certified_checkpoint<'a>(
    keys: impl Iterator<Item = &'a AuthorityKeyPair>,
    committee: Committee,
    seq_num: u64,
) -> CertifiedCheckpointSummary {
    let contents = CheckpointContents::new_with_causally_ordered_transactions(
        [ExecutionDigests::random()].into_iter(),
    );

    let summary = CheckpointSummary::new(
        committee.epoch,
        seq_num,
        0,
        &contents,
        None,
        GasCostSummary::default(),
        None,
        0,
    );

    let sign_infos: Vec<_> = keys
        .map(|k| {
            let name = k.public().into();

            SignedCheckpointSummary::sign(committee.epoch, &summary, k, name)
        })
        .collect();

    CertifiedCheckpointSummary::new(summary, sign_infos, &committee).expect("Cert is OK")
}

mod zk_login {
    use fastcrypto_zkp::bn254::{utils::big_int_str_to_bytes, zk_login::ZkLoginInputs};

    use super::*;

    fn get_inputs() -> ZkLoginInputs {
        thread_local! {
            static ZKLOGIN_INPUTS: ZkLoginInputs = ZkLoginInputs::from_json("{\"proof_points\":{\"pi_a\":[\"13051843614423432670927014216106415297302637061714876201698143724579485057433\",\"4502868464003469408525038047182455632737577658719986979336848385442638461715\",\"1\"],\"pi_b\":[[\"3766376969677702623227635810414794209012765519614143648451277080679068806865\",\"15274848924865252629859978623879023835428524488301134035498591572116063831502\"],[\"7946138017363758578225489333393267390091071305689481146893911497411367587561\",\"2088365154316551099212086738010689343716611426876233601699514012876088160321\"],[\"1\",\"0\"]],\"pi_c\":[\"5241274211994340733749437701839810664498991243102320350483342489924082173589\",\"15038895168244503564969393433273344359013838973322340723065298413943589759359\",\"1\"]},\"address_seed\":\"15454157267374145582481438333218897413377268576773635507925280300337575904853\",\"claims\":[{\"name\":\"iss\",\"value_base64\":\"wiaXNzIjoiaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyIiw\",\"index_mod_4\":2},{\"name\":\"aud\",\"value_base64\":\"yJhdWQiOiJyczFiaDA2NWk5eWE0eWR2aWZpeGw0a3NzMHVocHQiLC\",\"index_mod_4\":1}],\"header_base64\":\"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEifQ\"}").unwrap().init().unwrap();
        }
        ZKLOGIN_INPUTS.with(|a| a.clone())
    }

    pub fn get_zklogin_user_address() -> SuiAddress {
        thread_local! {
            static USER_ADDRESS: SuiAddress = {
                // Derive user address manually: Blake2b_256 hash of [zklogin_flag || bcs bytes of AddressParams || address seed in bytes])
                let mut hasher = DefaultHash::default();
                hasher.update([SignatureScheme::ZkLoginAuthenticator.flag()]);
                hasher.update(bcs::to_bytes(&get_inputs().get_address_params()).unwrap());
                hasher.update(big_int_str_to_bytes(get_inputs().get_address_seed()));
                SuiAddress::from_bytes(hasher.finalize().digest).unwrap()
            };
        }
        USER_ADDRESS.with(|a| *a)
    }

    fn get_zklogin_user_key() -> SuiKeyPair {
        SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([0; 32])))
    }

    pub fn make_zklogin_tx() -> (SuiAddress, Transaction, GenericSignature) {
        let data = make_transaction_data(get_zklogin_user_address());

        sign_zklogin_tx(data)
    }

    pub fn sign_zklogin_tx(data: TransactionData) -> (SuiAddress, Transaction, GenericSignature) {
        // Sign the user transaction with the user's ephemeral key.
        //let tx = make_transaction(user_address, &user_key, Intent::sui_transaction());

        let tx = Transaction::from_data_and_signer(
            data,
            Intent::sui_transaction(),
            vec![&get_zklogin_user_key()],
        );

        let s = match tx.inner().tx_signatures.first().unwrap() {
            GenericSignature::Signature(s) => s,
            _ => panic!("Expected a signature"),
        };

        // Construct the authenticator with all user submitted components.
        let authenticator = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
            get_inputs(),
            10,
            s.clone(),
        ));

        let tx = Transaction::new(SenderSignedData::new(
            tx.transaction_data().clone(),
            Intent::sui_transaction(),
            vec![authenticator.clone()],
        ));

        (get_zklogin_user_address(), tx, authenticator)
    }
}

pub fn keys() -> Vec<SuiKeyPair> {
    let mut seed = StdRng::from_seed([0; 32]);
    let kp1: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair_from_rng(&mut seed).1);
    let kp2: SuiKeyPair = SuiKeyPair::Secp256k1(get_key_pair_from_rng(&mut seed).1);
    let kp3: SuiKeyPair = SuiKeyPair::Secp256r1(get_key_pair_from_rng(&mut seed).1);
    vec![kp1, kp2, kp3]
}

pub fn make_upgraded_multisig_tx() -> Transaction {
    let keys = keys();
    let pk1 = &keys[0].public();
    let pk2 = &keys[1].public();
    let pk3 = &keys[2].public();

    let multisig_pk = MultiSigPublicKey::new(
        vec![pk1.clone(), pk2.clone(), pk3.clone()],
        vec![1, 1, 1],
        2,
    )
    .unwrap();
    let addr = SuiAddress::from(&multisig_pk);
    let tx = make_transaction(addr, &keys[0], Intent::sui_transaction());

    let msg = IntentMessage::new(Intent::sui_transaction(), tx.transaction_data().clone());
    let sig1 = Signature::new_secure(&msg, &keys[0]);
    let sig2 = Signature::new_secure(&msg, &keys[1]);

    // Any 2 of 3 signatures verifies ok.
    let multi_sig1 = MultiSig::combine(vec![sig1, sig2], multisig_pk).unwrap();
    Transaction::new(SenderSignedData::new(
        tx.transaction_data().clone(),
        Intent::sui_transaction(),
        vec![GenericSignature::MultiSig(multi_sig1)],
    ))
}

pub use zk_login::*;
