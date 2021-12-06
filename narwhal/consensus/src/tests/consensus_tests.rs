// Copyright(C) Facebook, Inc. and its affiliates.
use super::*;
use config::{Authority, PrimaryAddresses};
use crypto::{generate_keypair, SecretKey};
use primary::Header;
use rand::{rngs::StdRng, Rng, SeedableRng as _};
use std::collections::{BTreeSet, VecDeque};
use tokio::sync::mpsc::channel;

// Fixture
pub fn keys() -> Vec<(PublicKey, SecretKey)> {
    let mut rng = StdRng::from_seed([0; 32]);
    (0..4).map(|_| generate_keypair(&mut rng)).collect()
}

// Fixture
pub fn mock_committee(keys: &[PublicKey]) -> Committee {
    Committee {
        authorities: keys
            .iter()
            .map(|id| {
                (
                    *id,
                    Authority {
                        stake: 1,
                        primary: PrimaryAddresses {
                            primary_to_primary: "0.0.0.0:0".parse().unwrap(),
                            worker_to_primary: "0.0.0.0:0".parse().unwrap(),
                        },
                        workers: HashMap::default(),
                    },
                )
            })
            .collect(),
    }
}

// Fixture
pub fn mock_certificate(
    origin: PublicKey,
    round: Round,
    parents: BTreeSet<Digest>,
) -> (Digest, Certificate) {
    let certificate = Certificate {
        header: Header {
            author: origin,
            round,
            parents,
            ..Header::default()
        },
        ..Certificate::default()
    };
    (certificate.digest(), certificate)
}

// Creates one certificate per authority starting and finishing at the specified rounds (inclusive).
// Outputs a VecDeque of certificates (the certificate with higher round is on the front) and a set
// of digests to be used as parents for the certificates of the next round.
pub fn make_optimal_certificates(
    start: Round,
    stop: Round,
    initial_parents: &BTreeSet<Digest>,
    keys: &[PublicKey],
) -> (VecDeque<Certificate>, BTreeSet<Digest>) {
    make_certificates(start, stop, initial_parents, keys, 0.0)
}

pub fn make_certificates(
    start: Round,
    stop: Round,
    initial_parents: &BTreeSet<Digest>,
    keys: &[PublicKey],
    failure_probability: f64,
) -> (VecDeque<Certificate>, BTreeSet<Digest>) {
    let mut certificates = VecDeque::new();
    let mut parents = initial_parents.iter().cloned().collect::<BTreeSet<_>>();
    let mut next_parents = BTreeSet::new();

    fn this_cert_parents(ancestors: &BTreeSet<Digest>, failure_prob: f64) -> BTreeSet<Digest> {
        std::iter::from_fn(|| {
            let f: f64 = rand::thread_rng().gen();
            if f > failure_prob {
                Some(true)
            } else {
                Some(false)
            }
        })
        .take(ancestors.len())
        .zip(ancestors)
        .flat_map(|(parenthood, parent)| {
            if parenthood {
                Some(parent.clone())
            } else {
                None
            }
        })
        .collect::<BTreeSet<_>>()
    }

    for round in start..=stop {
        next_parents.clear();
        for name in keys {
            let this_cert_parents = this_cert_parents(&parents, failure_probability);

            let (digest, certificate) = mock_certificate(*name, round, this_cert_parents);
            certificates.push_back(certificate);
            next_parents.insert(digest);
        }
        parents = next_parents.clone();
    }
    (certificates, next_parents)
}

// Run for 4 dag rounds in ideal conditions (all nodes reference all other nodes). We should commit
// the leader of round 2.
#[tokio::test]
async fn commit_one() {
    // Make certificates for rounds 1 to 4.
    let keys: Vec<_> = keys().into_iter().map(|(x, _)| x).collect();

    // this should remain in scope for the test's duration
    let temp_dir = tempfile::tempdir().expect("Failed to open temporary directory");
    let consensus_db_path = temp_dir
        .into_path()
        .join(".consensus_db_test_commit_one")
        .display()
        .to_string();

    let genesis = Certificate::genesis(&mock_committee(&keys[..]))
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();
    let (mut certificates, next_parents) = make_optimal_certificates(1, 4, &genesis, &keys);

    // Make one certificate with round 5 to trigger the commits.
    let (_, certificate) = mock_certificate(keys[0], 5, next_parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = channel(1);
    let (tx_primary, mut rx_primary) = channel(1);
    let (tx_output, mut rx_output) = channel(1);
    Consensus::spawn(
        mock_committee(&keys[..]),
        /* gc_depth */ 50,
        consensus_db_path,
        rx_waiter,
        tx_primary,
        tx_output,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus. Only the last certificate should trigger
    // commits, so the task should not block.
    while let Some(certificate) = certificates.pop_front() {
        tx_waiter.send(certificate).await.unwrap();
    }

    // Ensure the first 4 ordered certificates are from round 1 (they are the parents of the committed
    // leader); then the leader's certificate should be committed.
    for _ in 1..=4 {
        let certificate = rx_output.recv().await.unwrap();
        assert_eq!(certificate.round(), 1);
    }
    let certificate = rx_output.recv().await.unwrap();
    assert_eq!(certificate.round(), 2);
}

// Run for 8 dag rounds with one dead node node (that is not a leader). We should commit the leaders of
// rounds 2, 4, and 6.
#[tokio::test]
async fn dead_node() {
    // Make the certificates.
    let mut keys: Vec<_> = keys().into_iter().map(|(x, _)| x).collect();
    keys.sort(); // Ensure we don't remove one of the leaders.
    let _ = keys.pop().unwrap();

    // this should remain in scope for the test's duration
    let temp_dir = tempfile::tempdir().expect("Failed to open temporary directory");
    let consensus_db_path = temp_dir
        .into_path()
        .join(".consensus_db_test_dead_node")
        .display()
        .to_string();

    let genesis = Certificate::genesis(&mock_committee(&keys[..]))
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let (mut certificates, _) = make_optimal_certificates(1, 9, &genesis, &keys);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = channel(1);
    let (tx_primary, mut rx_primary) = channel(1);
    let (tx_output, mut rx_output) = channel(1);
    Consensus::spawn(
        mock_committee(&keys[..]),
        /* gc_depth */ 50,
        consensus_db_path,
        rx_waiter,
        tx_primary,
        tx_output,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus.
    tokio::spawn(async move {
        while let Some(certificate) = certificates.pop_front() {
            tx_waiter.send(certificate).await.unwrap();
        }
    });

    // We should commit 3 leaders (rounds 2, 4, and 6).
    for i in 1..=15 {
        let certificate = rx_output.recv().await.unwrap();
        let expected = ((i - 1) / keys.len() as u64) + 1;
        assert_eq!(certificate.round(), expected);
    }
    let certificate = rx_output.recv().await.unwrap();
    assert_eq!(certificate.round(), 6);
}

// Run for 6 dag rounds. The leaders of round 2 does not have enough support, but the leader of
// round 4 does. The leader of rounds 2 and 4 should thus be committed upon entering round 6.
#[tokio::test]
async fn not_enough_support() {
    let mut keys: Vec<_> = keys().into_iter().map(|(x, _)| x).collect();
    keys.sort();

    // this should remain in scope for the test's duration
    let temp_dir = tempfile::tempdir().expect("Failed to open temporary directory");
    let consensus_db_path = temp_dir
        .into_path()
        .join(".consensus_db_test_not_enough_support")
        .display()
        .to_string();

    let genesis = Certificate::genesis(&mock_committee(&keys[..]))
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let mut certificates = VecDeque::new();

    // Round 1: Fully connected graph.
    let nodes: Vec<_> = keys.iter().cloned().take(3).collect();
    let (out, parents) = make_optimal_certificates(1, 1, &genesis, &nodes);
    certificates.extend(out);

    // Round 2: Fully connect graph. But remember the digest of the leader. Note that this
    // round is the only one with 4 certificates.
    let (leader_2_digest, certificate) = mock_certificate(keys[0], 2, parents.clone());
    certificates.push_back(certificate);

    let nodes: Vec<_> = keys.iter().cloned().skip(1).collect();
    let (out, mut parents) = make_optimal_certificates(2, 2, &parents, &nodes);
    certificates.extend(out);

    // Round 3: Only node 0 links to the leader of round 2.
    let mut next_parents = BTreeSet::new();

    let name = &keys[1];
    let (digest, certificate) = mock_certificate(*name, 3, parents.clone());
    certificates.push_back(certificate);
    next_parents.insert(digest);

    let name = &keys[2];
    let (digest, certificate) = mock_certificate(*name, 3, parents.clone());
    certificates.push_back(certificate);
    next_parents.insert(digest);

    let name = &keys[0];
    parents.insert(leader_2_digest);
    let (digest, certificate) = mock_certificate(*name, 3, parents.clone());
    certificates.push_back(certificate);
    next_parents.insert(digest);

    parents = next_parents.clone();

    // Rounds 4, 5, and 6: Fully connected graph.
    let nodes: Vec<_> = keys.iter().cloned().take(3).collect();
    let (out, parents) = make_optimal_certificates(4, 6, &parents, &nodes);
    certificates.extend(out);

    // Round 7: Send a single certificate to trigger the commits.
    let (_, certificate) = mock_certificate(keys[0], 7, parents);
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = channel(1);
    let (tx_primary, mut rx_primary) = channel(1);
    let (tx_output, mut rx_output) = channel(1);
    Consensus::spawn(
        mock_committee(&keys[..]),
        /* gc_depth */ 50,
        consensus_db_path,
        rx_waiter,
        tx_primary,
        tx_output,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus. Only the last certificate should trigger
    // commits, so the task should not block.
    while let Some(certificate) = certificates.pop_front() {
        tx_waiter.send(certificate).await.unwrap();
    }

    // We should commit 2 leaders (rounds 2 and 4).
    for _ in 1..=3 {
        let certificate = rx_output.recv().await.unwrap();
        assert_eq!(certificate.round(), 1);
    }
    for _ in 1..=4 {
        let certificate = rx_output.recv().await.unwrap();
        assert_eq!(certificate.round(), 2);
    }
    for _ in 1..=3 {
        let certificate = rx_output.recv().await.unwrap();
        assert_eq!(certificate.round(), 3);
    }
    let certificate = rx_output.recv().await.unwrap();
    assert_eq!(certificate.round(), 4);
}

// Run for 6 dag rounds. Node 0 (the leader of round 2) is missing for rounds 1 and 2,
// and reapers from round 3.
#[tokio::test]
async fn missing_leader() {
    let mut keys: Vec<_> = keys().into_iter().map(|(x, _)| x).collect();
    keys.sort();

    // this should remain in scope for the test's duration
    let temp_dir = tempfile::tempdir().expect("Failed to open temporary directory");
    let consensus_db_path = temp_dir
        .into_path()
        .join(".consensus_db_test_missing_leader")
        .display()
        .to_string();

    let genesis = Certificate::genesis(&mock_committee(&keys[..]))
        .iter()
        .map(|x| x.digest())
        .collect::<BTreeSet<_>>();

    let mut certificates = VecDeque::new();

    // Remove the leader for rounds 1 and 2.
    let nodes: Vec<_> = keys.iter().cloned().skip(1).collect();
    let (out, parents) = make_optimal_certificates(1, 2, &genesis, &nodes);
    certificates.extend(out);

    // Add back the leader for rounds 3, 4, 5 and 6.
    let (out, parents) = make_optimal_certificates(3, 6, &parents, &keys);
    certificates.extend(out);

    // Add a certificate of round 7 to commit the leader of round 4.
    let (_, certificate) = mock_certificate(keys[0], 7, parents.clone());
    certificates.push_back(certificate);

    // Spawn the consensus engine and sink the primary channel.
    let (tx_waiter, rx_waiter) = channel(1);
    let (tx_primary, mut rx_primary) = channel(1);
    let (tx_output, mut rx_output) = channel(1);
    Consensus::spawn(
        mock_committee(&keys[..]),
        /* gc_depth */ 50,
        consensus_db_path,
        rx_waiter,
        tx_primary,
        tx_output,
    );
    tokio::spawn(async move { while rx_primary.recv().await.is_some() {} });

    // Feed all certificates to the consensus. We should only commit upon receiving the last
    // certificate, so calls below should not block the task.
    while let Some(certificate) = certificates.pop_front() {
        tx_waiter.send(certificate).await.unwrap();
    }

    // Ensure the commit sequence is as expected.
    for _ in 1..=3 {
        let certificate = rx_output.recv().await.unwrap();
        assert_eq!(certificate.round(), 1);
    }
    for _ in 1..=3 {
        let certificate = rx_output.recv().await.unwrap();
        assert_eq!(certificate.round(), 2);
    }
    for _ in 1..=4 {
        let certificate = rx_output.recv().await.unwrap();
        assert_eq!(certificate.round(), 3);
    }
    let certificate = rx_output.recv().await.unwrap();
    assert_eq!(certificate.round(), 4);
}
