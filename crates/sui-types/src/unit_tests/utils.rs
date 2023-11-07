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
    let contents =
        CheckpointContents::new_with_digests_only_for_tests([ExecutionDigests::random()]);

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
    use shared_crypto::intent::PersonalMessage;

    use super::*;

    pub fn get_inputs() -> ZkLoginInputs {
        thread_local! {
        static ZKLOGIN_INPUTS: ZkLoginInputs = ZkLoginInputs::from_json("{\"proofPoints\":{\"a\":[\"17318089125952421736342263717932719437717844282410187957984751939942898251250\",\"11373966645469122582074082295985388258840681618268593976697325892280915681207\",\"1\"],\"b\":[[\"5939871147348834997361720122238980177152303274311047249905942384915768690895\",\"4533568271134785278731234570361482651996740791888285864966884032717049811708\"],[\"10564387285071555469753990661410840118635925466597037018058770041347518461368\",\"12597323547277579144698496372242615368085801313343155735511330003884767957854\"],[\"1\",\"0\"]],\"c\":[\"15791589472556826263231644728873337629015269984699404073623603352537678813171\",\"4547866499248881449676161158024748060485373250029423904113017422539037162527\",\"1\"]},\"issBase64Details\":{\"value\":\"wiaXNzIjoiaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyIiw\",\"indexMod4\":2},\"headerBase64\":\"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEifQ\"}", "20794788559620669596206457022966176986688727876128223628113916380927502737911").unwrap(); }
        ZKLOGIN_INPUTS.with(|a| a.clone())
    }

    pub fn get_zklogin_user_address() -> SuiAddress {
        thread_local! {
            static USER_ADDRESS: SuiAddress = {
                // Derive user address manually: Blake2b_256 hash of [zklogin_flag || iss_bytes_length || iss_bytes || address seed in bytes])
                let mut hasher = DefaultHash::default();
                hasher.update([SignatureScheme::ZkLoginAuthenticator.flag()]);
                let inputs = get_inputs();
                let iss_bytes = inputs.get_iss().as_bytes();
                hasher.update([iss_bytes.len() as u8]);
                hasher.update(iss_bytes);
                hasher.update(big_int_str_to_bytes(inputs.get_address_seed()).unwrap());
                SuiAddress::from_bytes(hasher.finalize().digest).unwrap()
            };
        }
        USER_ADDRESS.with(|a| *a)
    }

    fn get_zklogin_user_key() -> SuiKeyPair {
        SuiKeyPair::Ed25519(Ed25519KeyPair::generate(&mut StdRng::from_seed([0; 32])))
    }

    fn get_inputs_with_bad_address_seed() -> ZkLoginInputs {
        thread_local! {
        static ZKLOGIN_INPUTS: ZkLoginInputs = ZkLoginInputs::from_json("{\"proofPoints\":{\"a\":[\"17276311605393076686048412951904952585208929623427027497902331765285829154985\",\"2195957390349729412627479867125563520760023859523358729791332629632025124364\",\"1\"],\"b\":[[\"10285059021604767951039627893758482248204478992077021270802057708215366770814\",\"20086937595807139308592304218494658586282197458549968652049579308384943311509\"],[\"7481123765095657256931104563876569626157448050870256177668773471703520958615\",\"11912752790863530118410797223176516777328266521602785233083571774104055633375\"],[\"1\",\"0\"]],\"c\":[\"15742763887654796666500488588763616323599882100448686869458326409877111249163\",\"6112916537574993759490787691149241262893771114597679488354854987586060572876\",\"1\"]},\"issBase64Details\":{\"value\":\"wiaXNzIjoiaHR0cHM6Ly9pZC50d2l0Y2gudHYvb2F1dGgyIiw\",\"indexMod4\":2},\"headerBase64\":\"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IjEifQ\"}", "380704556853533152350240698167704405529973457670972223618755249929828551006").unwrap(); }
        ZKLOGIN_INPUTS.with(|a| a.clone())
    }

    pub fn get_legacy_zklogin_user_address() -> SuiAddress {
        thread_local! {
            static USER_ADDRESS: SuiAddress = {
                // Derive user address manually: Blake2b_256 hash of [zklogin_flag || iss_bytes_length || iss_bytes || address seed in bytes])
                let mut hasher = DefaultHash::default();
                hasher.update([SignatureScheme::ZkLoginAuthenticator.flag()]);
                let inputs = get_inputs_with_bad_address_seed();
                let iss_bytes = inputs.get_iss().as_bytes();
                hasher.update([iss_bytes.len() as u8]);
                hasher.update(iss_bytes);
                let bytes = big_int_str_to_bytes(inputs.get_address_seed()).unwrap();
                // The bytes from example address seed is 31 bytes, padded with 0 to 32 bytes.
                let mut padded = Vec::new();
                padded.extend(vec![0; 32 - bytes.len()]);
                padded.extend(bytes);
                hasher.update(padded);
                SuiAddress::from_bytes(hasher.finalize().digest).unwrap()
            };
        }
        USER_ADDRESS.with(|a| *a)
    }

    pub fn make_zklogin_tx(legacy: bool) -> (SuiAddress, Transaction, GenericSignature) {
        let data = if legacy {
            make_transaction_data(get_legacy_zklogin_user_address())
        } else {
            make_transaction_data(get_zklogin_user_address())
        };
        sign_zklogin_tx(data, legacy)
    }

    pub fn sign_zklogin_personal_msg(data: PersonalMessage) -> (SuiAddress, GenericSignature) {
        let inputs = get_inputs();
        let msg = IntentMessage::new(Intent::personal_message(), data);
        let s = Signature::new_secure(&msg, &get_zklogin_user_key());
        let authenticator =
            GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(inputs, 10, s));
        let address = get_zklogin_user_address();
        (address, authenticator)
    }

    pub fn sign_zklogin_tx(
        data: TransactionData,
        legacy: bool,
    ) -> (SuiAddress, Transaction, GenericSignature) {
        // Sign the user transaction with the user's ephemeral key.
        let tx = Transaction::from_data_and_signer(
            data,
            Intent::sui_transaction(),
            vec![&get_zklogin_user_key()],
        );

        let s = match tx.inner().tx_signatures.first().unwrap() {
            GenericSignature::Signature(s) => s,
            _ => panic!("Expected a signature"),
        };

        let inputs = if legacy {
            get_inputs_with_bad_address_seed()
        } else {
            get_inputs()
        };
        // Construct the authenticator with all user submitted components.
        let authenticator = GenericSignature::ZkLoginAuthenticator(ZkLoginAuthenticator::new(
            inputs,
            10,
            s.clone(),
        ));

        let tx = Transaction::new(SenderSignedData::new(
            tx.transaction_data().clone(),
            Intent::sui_transaction(),
            vec![authenticator.clone()],
        ));
        let addr = if legacy {
            get_legacy_zklogin_user_address()
        } else {
            get_zklogin_user_address()
        };
        (addr, tx, authenticator)
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
