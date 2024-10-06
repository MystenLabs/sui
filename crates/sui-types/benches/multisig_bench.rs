use criterion::{criterion_group, criterion_main, Criterion};
use shared_crypto::intent::{Intent, IntentMessage, PersonalMessage};
use sui_types::{base_types::SuiAddress, crypto::{get_key_pair, Signature, SuiKeyPair}, multisig::{MultiSig, MultiSigPublicKey}, signature::{AuthenticatorTrait, GenericSignature}, signature_verification::VerifiedDigestCache};
use std::sync::Arc;

fn bench_multisig_signing(c: &mut Criterion) {
    let kp1: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let kp2: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let kp3: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);

    let pk1 = kp1.public();
    let pk2 = kp2.public();
    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2, kp3.public()], vec![1, 1, 1], 2).unwrap();
    
    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );

    c.bench_function("multisig signing", |b| {
        b.iter(|| {
            let sig1: GenericSignature = Signature::new_secure(&msg, &kp1).into();
            let sig2 = Signature::new_secure(&msg, &kp2).into();
            MultiSig::combine(vec![sig1, sig2], multisig_pk.clone()).unwrap();
        });
    });
}

fn bench_multisig_verification(c: &mut Criterion) {
    let kp1: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let kp2: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);
    let kp3: SuiKeyPair = SuiKeyPair::Ed25519(get_key_pair().1);

    let pk1 = kp1.public();
    let pk2 = kp2.public();
    let multisig_pk = MultiSigPublicKey::new(vec![pk1, pk2, kp3.public()], vec![1, 1, 1], 2).unwrap();
    let multisig_addr = SuiAddress::from(&multisig_pk);

    let msg = IntentMessage::new(
        Intent::sui_transaction(),
        PersonalMessage {
            message: "Hello".as_bytes().to_vec(),
        },
    );

    let sig1: GenericSignature = Signature::new_secure(&msg, &kp1).into();
    let sig2 = Signature::new_secure(&msg, &kp2).into();
    let multisig = MultiSig::combine(vec![sig1, sig2], multisig_pk.clone()).unwrap();

    c.bench_function("multisig verification", |b| {
        b.iter(|| {
            multisig.verify_claims(
                &msg,
                multisig_addr,
                &Default::default(),
                Arc::new(VerifiedDigestCache::new_empty()),
            )
            .unwrap();
        });
    });
}

criterion_group!(benches, bench_multisig_signing, bench_multisig_verification);
criterion_main!(benches);