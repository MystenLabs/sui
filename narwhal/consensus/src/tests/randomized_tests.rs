// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::bullshark::Bullshark;
use crate::consensus::ConsensusProtocol;
use crate::consensus::ConsensusState;
use crate::consensus_utils::make_consensus_store;
use crate::metrics::ConsensusMetrics;
use config::{Committee, Stake};
use crypto::PublicKey;
use fastcrypto::hash::Hash;
use fastcrypto::hash::HashFunction;
use prometheus::Registry;
use rand::distributions::Bernoulli;
use rand::distributions::Distribution;
use rand::prelude::SliceRandom;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::ops::RangeInclusive;
use std::sync::Arc;
use test_utils::mock_certificate;
use test_utils::CommitteeFixture;
#[allow(unused_imports)]
use tokio::sync::mpsc::channel;
use types::Round;
use types::{Certificate, CertificateDigest};

#[derive(Copy, Clone)]
pub struct FailureModes {
    // The probability of having failures per round. The failures should
    // be <=f , otherwise no DAG could be created. The provided number gives the probability of having
    // failures up to f. Ex for input `failures_probability = 0.2` it means we'll have 20% change of
    // having failures up to 33% of the nodes.
    pub nodes_failure_probability: f64,

    // The percentage of slow nodes we want to introduce to our sample. Basically a slow node is one
    // that might be able to produce certificates, but those are never get referenced by others. That has
    // as an effect that when they are leaders might also not get enough support - or no support at all.
    // For example, a value of 0.2 means that we want up to 20% of our nodes to behave as slow nodes.
    pub slow_nodes_percentage: f64,

    // The probability of failing to include a slow node certificate to from the certificates of next
    // round. For example a value of 0.1 means that 10% of the time fail get referenced by the
    // certificates of the next round.
    pub slow_nodes_failure_probability: f64,
}

struct ExecutionPlan {
    certificates: Vec<Certificate>,
}

impl ExecutionPlan {
    fn hash(&self) -> [u8; crypto::DIGEST_LENGTH] {
        let mut hasher = crypto::DefaultHashFunction::new();
        self.certificates.iter().for_each(|c| {
            hasher.update(c.digest());
        });
        hasher.finalize().into()
    }
}

#[tokio::test]
#[ignore]
async fn bullshark_randomised_tests() {
    // Configuration regarding the randomized tests. The tests will run for different values
    // on the below parameters to increase the different cases we can generate.

    // A range of gc_depth to be used
    const GC_DEPTH: RangeInclusive<Round> = 4..=15;
    // A range of the committee size to be used
    const COMMITTEE_SIZE: RangeInclusive<usize> = 4..=8;
    // A range of rounds for which we will create DAGs
    const DAG_ROUNDS: RangeInclusive<Round> = 8..=15;
    // The number of different execution plans to be created and tested against for every generated DAG
    const EXECUTION_PLANS: u64 = 10_000;
    // The number of DAGs that should be generated and tested against for every set of properties.
    const DAGS_PER_SETUP: u64 = 1_000;
    // DAGs will be created for these failure modes
    let failure_modes: Vec<FailureModes> = vec![
        // No failures
        FailureModes {
            nodes_failure_probability: 0.0,
            slow_nodes_percentage: 0.0,
            slow_nodes_failure_probability: 0.0,
        },
        // Some failures
        FailureModes {
            nodes_failure_probability: 0.05,     // 5%
            slow_nodes_percentage: 0.05,         // 5%
            slow_nodes_failure_probability: 0.3, // 30%
        },
        // Severe failures
        FailureModes {
            nodes_failure_probability: 0.0,      // 0%
            slow_nodes_percentage: 0.2,          // 20%
            slow_nodes_failure_probability: 0.7, // 70%
        },
    ];

    let mut run_id = 0;
    for committee_size in COMMITTEE_SIZE {
        for gc_depth in GC_DEPTH {
            for dag_rounds in DAG_ROUNDS {
                for _ in 0..DAGS_PER_SETUP {
                    for mode in &failure_modes {
                        // we want to skip this test as gc_depth will never be enforced
                        if gc_depth > dag_rounds {
                            continue;
                        }

                        run_id += 1;

                        // Create a randomized DAG
                        let (certificates, committee) =
                            generate_randomised_dag(committee_size, dag_rounds, run_id, *mode);

                        // Now provide the DAG to create execution plans, run them via consensus
                        // and compare output against each other to ensure they are the same.
                        generate_and_run_execution_plans(
                            certificates,
                            EXECUTION_PLANS,
                            committee,
                            gc_depth,
                            dag_rounds,
                            run_id,
                        );
                    }
                }
            }
        }
    }
}

// Creates a DAG with the known parameters but with some sort of randomness
// to ensure that the DAG will create:
// * weak references to leaders
// * missing leaders
// * missing certificates

// Note: the slow nodes precede of the failures_probability - meaning that first we calculate the
// failures per round and then the behaviour of the slow nodes to ensure that we'll always produce
// 2f+1 certificates per round.
fn generate_randomised_dag(
    committee_size: usize,
    number_of_rounds: Round,
    seed: u64,
    modes: FailureModes,
) -> (VecDeque<Certificate>, Committee) {
    // Create an RNG to share for the committee creation
    let rand = StdRng::seed_from_u64(seed);

    let fixture = CommitteeFixture::builder()
        .committee_size(NonZeroUsize::new(committee_size).unwrap())
        .rng(rand)
        .build();
    let committee: Committee = fixture.committee();
    let genesis = Certificate::genesis(&committee);

    // Create a known DAG
    let (original_certificates, _last_round) =
        make_certificates_with_parameters(seed, &committee, 1..=number_of_rounds, genesis, modes);

    (original_certificates, committee)
}

/// This method is creating DAG using the following quality properties under consideration:
/// * nodes that don't create certificates at all for some rounds (failures)
/// * leaders that don't get enough support (f+1) for their immediate round
/// * slow nodes - nodes that create certificates but those might not referenced by nodes of
/// subsequent rounds.
pub fn make_certificates_with_parameters(
    seed: u64,
    committee: &Committee,
    range: RangeInclusive<Round>,
    initial_parents: Vec<Certificate>,
    modes: FailureModes,
) -> (VecDeque<Certificate>, Vec<Certificate>) {
    let mut rand = StdRng::seed_from_u64(seed);

    //Pick the slow nodes - ensure we don't have more than 33% of slow nodes
    assert!(modes.slow_nodes_percentage <= 0.33, "Slow nodes can't be more than 33% of total nodes - otherwise we'll basically simulate a consensus stall");

    let mut keys: Vec<PublicKey> = committee
        .authorities()
        .map(|(key, _)| key.clone())
        .collect();

    // Now shuffle authorities and pick the slow nodes, if should exist
    keys.shuffle(&mut rand);

    // Step 1 - determine the slow nodes , assuming those should exist
    let slow_node_keys: Vec<(PublicKey, f64)> = {
        let num_of_slow_nodes =
            (committee.total_stake() as f64 * modes.slow_nodes_percentage) as Stake;
        let s = num_of_slow_nodes.min(committee.validity_threshold() - 1);

        keys.iter()
            .take(s as usize)
            .map(|k| (k.clone(), 1.0 - modes.slow_nodes_failure_probability))
            .collect()
    };

    println!("Slow nodes: {:?}", slow_node_keys);

    let mut certificates = VecDeque::new();
    let mut parents = initial_parents;
    let mut next_parents = Vec::new();
    let mut certificates_per_round: HashMap<Round, Vec<Certificate>> = HashMap::new();

    parents.iter().for_each(|c| {
        certificates_per_round
            .entry(c.round())
            .or_default()
            .push(c.clone());
    });

    for round in range {
        next_parents.clear();

        let mut total_failures = 0;

        // shuffle keys to introduce extra randomness
        keys.shuffle(&mut rand);

        for name in keys.iter() {
            let current_parents = parents.clone();

            // Step 2 -- introduce failures (assuming those are enabled)
            // We disable the failure probability if we have already reached the maximum number
            // of allowed failures (f)
            let should_fail = if total_failures + 1 == committee.validity_threshold() {
                false
            } else {
                let b = Bernoulli::new(modes.nodes_failure_probability).unwrap();
                b.sample(&mut rand)
            };

            if should_fail {
                total_failures += 1;
                continue;
            }

            // Step 3 -- figure out the parents taking into account the slow nodes - assuming they
            // are provide such.
            let parent_digests = test_utils::this_cert_parents_with_slow_nodes(
                name,
                current_parents.clone(),
                slow_node_keys.as_slice(),
                &mut rand,
            );
            let mut parent_digests: Vec<CertificateDigest> = parent_digests.into_iter().collect();

            // Step 3 -- references to previous round
            // Now from the rest of current_parents, pick a random number - uniform - to how many
            // should create references to. It should strictly be between [2f+1..3f+1].
            let num_of_parents_to_pick =
                rand.gen_range(committee.quorum_threshold()..=committee.total_stake());

            // shuffle the parents
            parent_digests.shuffle(&mut rand);

            // now keep only the num_of_parents_to_pick
            let parents_digests = parent_digests
                .into_iter()
                .take(num_of_parents_to_pick as usize)
                .collect();

            // Now create the certificate with the provided parents
            let (_, certificate) =
                mock_certificate(committee, name.clone(), round, parents_digests);

            // group certificates by round for easy access
            certificates_per_round
                .entry(certificate.round())
                .or_default()
                .push(certificate.clone());

            certificates.push_back(certificate.clone());
            next_parents.push(certificate);
        }
        parents = next_parents.clone();
    }

    // sanity check - before return, ensure that we have at least 2f+1 certificates per round and
    // not certificate exists (except the genesis ones) whose parent is missing.
    certificates_per_round
        .iter()
        .for_each(|(round, round_certs)| {
            assert!(round_certs.len() >= committee.quorum_threshold() as usize);

            if *round > 0 {
                let parents = certificates_per_round.get(&(round - 1)).unwrap();
                round_certs
                    .iter()
                    .flat_map(|c| c.header.parents.clone())
                    .for_each(|digest| {
                        parents
                            .iter()
                            .find(|c| c.digest() == digest)
                            .unwrap_or_else(|| {
                                panic!(
                                    "Certificate with digest {} should be found in parents",
                                    digest
                                )
                            });
                    });
            }
        });

    (certificates, next_parents)
}

/// Creates various execution plans (`test_iterations` in total) by permuting the order we feed the
/// DAG certificates to consensus and compare the output to ensure is the same.
fn generate_and_run_execution_plans(
    original_certificates: VecDeque<Certificate>,
    test_iterations: u64,
    committee: Committee,
    gc_depth: Round,
    dag_rounds: Round,
    run_id: u64,
) {
    let mut executed_plans = HashSet::new();
    let mut committed_certificates = Vec::new();

    // Create a single store to be re-used across Bullshark instances to avoid hitting
    // a "too many files open" issue.
    let store = make_consensus_store(&test_utils::temp_dir());

    for i in 0..test_iterations {
        // clear store before using for next test
        store.clear().unwrap();

        let seed = (i + 1) * run_id;

        let plan = create_execution_plan(original_certificates.clone(), seed);

        let hash = plan.hash();
        if !executed_plans.insert(hash) {
            println!("Skipping plan with seed {}, same executed already", seed);
            continue;
        }

        // Now create a new Bullshark engine
        const NUM_SUB_DAGS_PER_SCHEDULE: u64 = 100;

        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
        let mut state = ConsensusState::new(metrics.clone(), &committee, gc_depth);
        let mut bullshark = Bullshark::new(
            committee.clone(),
            store.clone(),
            metrics,
            NUM_SUB_DAGS_PER_SCHEDULE,
        );

        let mut inserted_certificates = HashSet::new();

        let mut plan_committed_certificates = Vec::new();
        for c in plan.certificates {
            //print!("R {} - D {}", c.round(), c.digest());

            // A sanity check that we indeed attempt to send to Bullshark a certificate
            // whose parents have already been inserted.
            if c.round() > 1 {
                for parent in &c.header.parents {
                    assert!(inserted_certificates.contains(parent));
                }
            }
            inserted_certificates.insert(c.digest());

            // Now commit one by one the certificates and gather the results
            let (_outcome, committed_sub_dags) =
                bullshark.process_certificate(&mut state, c).unwrap();
            //println!(" -> Outcome: {:?}", outcome);
            for sub_dag in committed_sub_dags {
                plan_committed_certificates.extend(sub_dag.certificates);
            }
        }

        // Compare the results with the previously executed plan results
        if committed_certificates.is_empty() {
            committed_certificates = plan_committed_certificates.clone();
        } else {
            assert_eq!(
                committed_certificates,
                plan_committed_certificates,
                "Fork detected in plans for seed={}, rounds={}, committee={}, gc_depth={}",
                seed,
                dag_rounds,
                committee.authorities.len(),
                gc_depth
            );
        }

        println!(
            "Successfully committed plan with seed {} for rounds={}, committee={}, gc_depth={}",
            seed,
            dag_rounds,
            committee.authorities.len(),
            gc_depth
        );
    }
}

/// This method is accepting a list of certificates that have been created to represent a valid
/// DAG and puts them in a causally valid order to be sent to consensus but different than just
/// sending them round by round, so we can simulate more real life scenarios.
/// Basically it is creating an execution plan. A seed value is provided to be used in a random
/// function in order to perform random permutations when creating the sequence to help construct
/// different paths.
/// Using Kahn's DAG topological sort algorithm, we basically try to sort the certificate DAG
/// <https://en.wikipedia.org/wiki/Topological_sorting> always respecting the causal order of
/// certificates - meaning for every certificate on round R, we must first have  submitted all
/// parent certificates of round R-1.
fn create_execution_plan(
    certificates: impl IntoIterator<Item = Certificate> + Clone,
    seed: u64,
) -> ExecutionPlan {
    // Initialise the source of randomness
    let mut rand = StdRng::seed_from_u64(seed);

    // Create a map of digest -> certificate
    let digest_to_certificate: HashMap<CertificateDigest, Certificate> = certificates
        .clone()
        .into_iter()
        .map(|c| (c.digest(), c))
        .collect();

    // To model the DAG in form of edges and vertexes build an adjacency matrix.
    // The matrix will capture the dependencies between the parent certificates --> children certificates.
    // This is important because the algorithm ensures that no children will be added to the final list
    // unless all their dependencies (parent certificates) have first been added earlier - so we
    // respect the causal order.
    let mut adjacency_parent_to_children: HashMap<CertificateDigest, Vec<CertificateDigest>> =
        HashMap::new();

    // The nodes that have no incoming edges/dependencies (parent certificates) - initially are the certificates of
    // round 1 (we have no parents)
    let mut nodes_without_dependencies = Vec::new();

    for certificate in certificates {
        // for the first round of certificates we don't want to include their parents, as we won't
        // have them available anyways - so we want those to be our roots.
        if certificate.round() > 1 {
            for parent in &certificate.header.parents {
                adjacency_parent_to_children
                    .entry(*parent)
                    .or_default()
                    .push(certificate.digest());
            }
        } else {
            nodes_without_dependencies.push(certificate.digest());
        }
    }

    // The list that will keep the "sorted" certificates
    let mut sorted = Vec::new();

    while !nodes_without_dependencies.is_empty() {
        // randomize the pick from nodes_without_dependencies to get a different result
        let index = rand.gen_range(0..nodes_without_dependencies.len());

        let node = nodes_without_dependencies.remove(index);
        sorted.push(node);

        // now get their children references - if they have none then this is a certificate of last round
        if let Some(mut children) = adjacency_parent_to_children.remove(&node) {
            // shuffle the children here again to create a different execution plan
            children.shuffle(&mut rand);

            while !children.is_empty() {
                let c = children.pop().unwrap();

                // has this children any other dependencies (certificate parents that have not been
                // already sorted)? If not, then add it to the candidate of nodes without incoming edges.
                let has_more_dependencies = adjacency_parent_to_children
                    .iter()
                    .any(|(_, entries)| entries.contains(&c));

                if !has_more_dependencies {
                    nodes_without_dependencies.push(c);
                }
            }
        }
    }

    assert!(
        adjacency_parent_to_children.is_empty(),
        "By now no edge should be left!"
    );

    let sorted = sorted
        .into_iter()
        .map(|c| digest_to_certificate.get(&c).unwrap().clone())
        .collect();

    ExecutionPlan {
        certificates: sorted,
    }
}
