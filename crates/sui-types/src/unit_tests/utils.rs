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
    zk_login_util::AddressParams,
};
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::hash::HashFunction;
use fastcrypto::traits::KeyPair as KeypairTraits;
use fastcrypto::traits::ToFromBytes;
use fastcrypto_zkp::bn254::zk_login::{
    big_int_str_to_bytes, AuxInputs, OAuthProvider, PublicInputs, SupportedKeyClaim, ZkLoginProof,
};
use rand::rngs::StdRng;
use rand::SeedableRng;
use shared_crypto::intent::{Intent, IntentMessage};
use std::collections::BTreeMap;

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
    use super::*;

    fn get_proof() -> ZkLoginProof {
        thread_local! {
            static PROOF: ZkLoginProof = ZkLoginProof::from_json("{\"pi_a\":[\"20481687889574648566428019854107422666251563600790506685694911259823357210233\",\"11033765255561191214948060245394777764506124769509578847678858254198587195988\",\"1\"],\"pi_b\":[[\"13849903711561313829934917969652092838978156769404699511299089312160663498036\",\"17165083942449254199283169058554211190103242687213364739060521395815615321163\"],[\"13911193808133271991822994609120431485365326128265885756983411624230351985366\",\"18294503277588808632327215611840242319415026224438195239817828865313916798900\"],[\"1\",\"0\"]],\"pi_c\":[\"2745104594185654286823222462448377902592393467516853387351618591034524080250\",\"6061284742000900942330610155574919999955182564883105081721057622928771176724\",\"1\"],\"protocol\":\"groth16\"}").unwrap();
        }
        PROOF.with(|p| p.clone())
    }

    fn get_aux_inputs() -> AuxInputs {
        thread_local! {
            static AUX_INPUTS: AuxInputs = AuxInputs::from_json("{\"addr_seed\":\"15981857537914003189887860118363233571575210046307592386608444813549663761927\",\"eph_public_key\":[\"166965004900001969288935757320344789203\",\"305091191138823352551463131979024497360\"],\"jwt_sha2_hash\":[\"212488216193738404698731705976768876226\",\"265684893115222891384798683500367647809\"],\"jwt_signature\":\"bfHHlvI1WmU-WfvQ9WdLQTbERXIlVUYn92hcMlP5mMhso4l5SVwh29DRQu_CJ0Cnmi1gSdUtzn1KFu1c2eFaFVW5ApbGWdAwk7e9pOpi3AoId_W9GN8Nnjp9EogsqTFQwHYz2Pbz7KWYNIGLwq1KZAsk0cixA3PwtJPZHnB4tXF9BGAS4XWDpK4hjqGCJYiQV1labncedbjxbxl7j0JtEE-JgxA4ymgx_OR4azKIjRPoGfT-ufIhEOKAD7p_auW6CNE-8v8VEX20REWNV3iw2WYTqpCGRqVTl92pslxRdU2eEcCa-X0P6_xtvFZVs3AwcRjbePS1ivp-MFqrnhmxSw\",\"key_claim_name\":\"sub\",\"masked_content\":[101, 121, 74, 104, 98, 71, 99, 105, 79, 105, 74, 83, 85, 122, 73, 49, 78, 105, 73, 115, 73, 109, 116, 112, 90, 67, 73, 54, 73, 106, 89, 119, 79, 68, 78, 107, 90, 68, 85, 53, 79, 68, 69, 50, 78, 122, 78, 109, 78, 106, 89, 120, 90, 109, 82, 108, 79, 87, 82, 104, 90, 84, 89, 48, 78, 109, 73, 50, 90, 106, 65, 122, 79, 68, 66, 104, 77, 68, 69, 48, 78, 87, 77, 105, 76, 67, 74, 48, 101, 88, 65, 105, 79, 105, 74, 75, 86, 49, 81, 105, 102, 81, 46, 61, 121, 74, 112, 99, 51, 77, 105, 79, 105, 74, 111, 100, 72, 82, 119, 99, 122, 111, 118, 76, 50, 70, 106, 89, 50, 57, 49, 98, 110, 82, 122, 76, 109, 100, 118, 98, 50, 100, 115, 90, 83, 53, 106, 98, 50, 48, 105, 76, 67, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 67, 74, 104, 100, 87, 81, 105, 79, 105, 73, 53, 78, 68, 89, 51, 77, 122, 69, 122, 78, 84, 73, 121, 78, 122, 89, 116, 99, 71, 115, 49, 90, 50, 120, 106, 90, 122, 104, 106, 99, 87, 56, 122, 79, 71, 53, 107, 89, 106, 77, 53, 97, 68, 100, 113, 77, 68, 107, 122, 90, 110, 66, 122, 99, 71, 104, 49, 99, 51, 85, 117, 89, 88, 66, 119, 99, 121, 53, 110, 98, 50, 57, 110, 98, 71, 86, 49, 99, 50, 86, 121, 89, 50, 57, 117, 100, 71, 86, 117, 100, 67, 53, 106, 98, 50, 48, 105, 76, 67, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 61, 128, 0, 0, 0, 0, 0, 0, 0, 0, 21, 168, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],\"max_epoch\":197,\"num_sha2_blocks\":11,\"payload_len\":590,\"payload_start_index\":103}").unwrap();
        }

        AUX_INPUTS.with(|a| a.clone())
    }

    fn get_public_inputs() -> PublicInputs {
        thread_local! {
            static PUBLIC_INPUTS: PublicInputs = PublicInputs::from_json(
                "[\"17943667729617637440509062721605164279479613242834309109747499529639666176396\"]",
            )
            .unwrap();
        }
        PUBLIC_INPUTS.with(|p| p.clone())
    }

    pub fn get_zklogin_user_address() -> SuiAddress {
        thread_local! {
            static USER_ADDRESS: SuiAddress = {
                // Derive user address manually: Blake2b_256 hash of [zklogin_flag || address seed in bytes || bcs bytes of AddressParams])
                let mut hasher = DefaultHash::default();
                hasher.update([SignatureScheme::ZkLoginAuthenticator.flag()]);
                let address_params = AddressParams::new(
                    OAuthProvider::Google.get_config().0.to_owned(),
                    SupportedKeyClaim::Sub.to_string(),
                );
                hasher.update(bcs::to_bytes(&address_params).unwrap());
                hasher.update(big_int_str_to_bytes(get_aux_inputs().get_address_seed()));
                SuiAddress::from_bytes(hasher.finalize().digest).unwrap()
            };
        }
        USER_ADDRESS.with(|a| *a)
    }

    fn get_zklogin_user_key() -> SuiKeyPair {
        SuiKeyPair::Ed25519(
            Ed25519KeyPair::from_bytes(
                &Base64::decode("a3R0jvXpEziZLHsbX1DogdyGm8AK87HScEK+JJHwaV8=").unwrap(),
            )
            .unwrap(),
        )
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
            get_proof(),
            get_public_inputs(),
            get_aux_inputs(),
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
