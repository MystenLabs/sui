// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::{Authority, Committee, Epoch, PrimaryAddresses, WorkerAddresses};
use crypto::{
    ed25519::{Ed25519KeyPair, Ed25519PublicKey},
    traits::{KeyPair, Signer},
    Digest, Hash,
};
use primary::PrimaryWorkerMessage;
use rand::{prelude::StdRng, SeedableRng};
use serde_reflection::{Registry, Result, Samples, Tracer, TracerConfig};
use std::{fs::File, io::Write};
use structopt::{clap::arg_enum, StructOpt};
use types::{
    Batch, BatchDigest, Certificate, CertificateDigest, Header, HeaderDigest,
    ReconfigureNotification, WorkerPrimaryError, WorkerPrimaryMessage,
};

fn get_registry() -> Result<Registry> {
    let mut tracer = Tracer::new(TracerConfig::default());
    let mut samples = Samples::new();
    // 1. Record samples for types with custom deserializers.
    // We want to call
    // tracer.trace_value(&mut samples, ...)?;
    // with all the base types contained in messages, especially the ones with custom serializers;
    // or involving generics (see [serde_reflection documentation](https://docs.rs/serde-reflection/latest/serde_reflection/)).
    let mut rng = StdRng::from_seed([0; 32]);
    let kp = Ed25519KeyPair::generate(&mut rng);
    let pk = kp.public().clone();

    tracer.trace_value(&mut samples, &pk)?;

    let msg = b"Hello world!";
    let signature = kp.try_sign(msg).unwrap();
    tracer.trace_value(&mut samples, &signature)?;

    // Trace the correspondng header
    let keys: Vec<_> = (0..4).map(|_| Ed25519KeyPair::generate(&mut rng)).collect();
    let committee = Committee {
        epoch: Epoch::default(),
        authorities: keys
            .iter()
            .enumerate()
            .map(|(i, kp)| {
                let id = kp.public();
                let primary = PrimaryAddresses {
                    primary_to_primary: format!("/ip4/127.0.0.1/tcp/{}/http", 100 + i)
                        .parse()
                        .unwrap(),
                    worker_to_primary: format!("/ip4/127.0.0.1/tcp/{}/http", 200 + i)
                        .parse()
                        .unwrap(),
                };
                let workers = vec![(
                    0,
                    WorkerAddresses {
                        primary_to_worker: format!("/ip4/127.0.0.1/tcp/{}/http", 300 + i)
                            .parse()
                            .unwrap(),
                        transactions: format!("/ip4/127.0.0.1/tcp/{}/http", 400 + i)
                            .parse()
                            .unwrap(),
                        worker_to_worker: format!("/ip4/127.0.0.1/tcp/{}/http", 500 + i)
                            .parse()
                            .unwrap(),
                    },
                )]
                .into_iter()
                .collect();
                (
                    id.clone(),
                    Authority {
                        stake: 1,
                        primary,
                        workers,
                    },
                )
            })
            .collect(),
    };

    let certificates: Vec<Certificate<Ed25519PublicKey>> = Certificate::genesis(&committee);

    // The values have to be "complete" in a data-centric sense, but not "correct" cryptographically.
    let mut header = Header {
        author: kp.public().clone(),
        round: 1,
        payload: (0..4u32).map(|wid| (BatchDigest([0u8; 32]), wid)).collect(),
        parents: certificates.iter().map(|x| x.digest()).collect(),
        ..Header::default()
    };

    let header_digest = header.digest();
    header = Header {
        id: header_digest,
        signature: kp.sign(Digest::from(header_digest).as_ref()),
        ..header
    };
    let certificate = Certificate {
        header: header.clone(),
        votes: vec![(pk.clone(), signature.clone())],
    };

    tracer.trace_value(&mut samples, &header)?;
    tracer.trace_value(&mut samples, &certificate)?;

    let cleanup = PrimaryWorkerMessage::<Ed25519PublicKey>::Cleanup(1u64);
    let request_batch =
        PrimaryWorkerMessage::<Ed25519PublicKey>::RequestBatch(BatchDigest([0u8; 32]));
    let delete_batch =
        PrimaryWorkerMessage::<Ed25519PublicKey>::DeleteBatches(vec![BatchDigest([0u8; 32])]);
    let sync = PrimaryWorkerMessage::Synchronize(vec![BatchDigest([0u8; 32])], pk.clone());
    let reconfigure =
        PrimaryWorkerMessage::Reconfigure(ReconfigureNotification::NewCommittee(committee));
    tracer.trace_value(&mut samples, &cleanup)?;
    tracer.trace_value(&mut samples, &request_batch)?;
    tracer.trace_value(&mut samples, &delete_batch)?;
    tracer.trace_value(&mut samples, &sync)?;
    tracer.trace_value(&mut samples, &reconfigure)?;

    // 2. Trace the main entry point(s) + every enum separately.
    tracer.trace_type::<Batch>(&samples)?;
    tracer.trace_type::<BatchDigest>(&samples)?;
    tracer.trace_type::<HeaderDigest>(&samples)?;
    tracer.trace_type::<CertificateDigest>(&samples)?;

    // Caveat: the following (trace_type) won't work, but not because of generics.
    //
    // Generic types instantiated multiple times in the same tracing session requires a work around.
    // https://docs.rs/serde-reflection/latest/serde_reflection/#features-and-limitations
    // but here we should be fine.
    //
    // This doesn't work because of the custom ser/de in PublicKey, which carries through to most top-level messages
    //
    // tracer.trace_type::<Header<Ed25519PublicKey>>(&samples)?;
    // tracer.trace_type::<Certificate<Ed25519PublicKey>>(&samples)?;
    // tracer.trace_type::<PrimaryWorkerMessage<Ed25519PublicKey>>(&samples)?;

    // The final entry points that we must document
    tracer.trace_type::<WorkerPrimaryMessage<Ed25519PublicKey>>(&samples)?;
    tracer.trace_type::<WorkerPrimaryError>(&samples)?;
    tracer.registry()
}

arg_enum! {
#[derive(Debug, StructOpt, Clone, Copy)]
enum Action {
    Print,
    Test,
    Record,
}
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Narwhal format generator",
    about = "Trace serde (de)serialization to generate format descriptions for Narwhal types"
)]
struct Options {
    #[structopt(possible_values = &Action::variants(), default_value = "Print", case_insensitive = true)]
    action: Action,
}

const FILE_PATH: &str = "node/tests/staged/narwhal.yaml";

fn main() {
    let options = Options::from_args();
    let registry = get_registry().unwrap();
    match options.action {
        Action::Print => {
            let content = serde_yaml::to_string(&registry).unwrap();
            println!("{content}");
        }
        Action::Record => {
            let content = serde_yaml::to_string(&registry).unwrap();
            let mut f = File::create(FILE_PATH).unwrap();
            writeln!(f, "{}", content).unwrap();
        }
        Action::Test => {
            // If this test fails, run the following command from the folder `node`:
            // cargo -q run --example generate-format -- print > tests/staged/narwhal.yaml
            let reference = std::fs::read_to_string(FILE_PATH).unwrap();
            let reference: Registry = serde_yaml::from_str(&reference).unwrap();
            pretty_assertions::assert_eq!(reference, registry);
        }
    }
}
