// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::bullshark::Bullshark;
use crate::consensus::ConsensusProtocol;
use crate::consensus::ConsensusState;
use crate::consensus_utils::make_consensus_store;
use crate::consensus_utils::NUM_SUB_DAGS_PER_SCHEDULE;
use crate::metrics::ConsensusMetrics;
use config::{Authority, AuthorityIdentifier, Committee, Stake};
use fastcrypto::hash::Hash;
use fastcrypto::hash::HashFunction;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use prometheus::Registry;
use rand::distributions::Bernoulli;
use rand::distributions::Distribution;
use rand::prelude::SliceRandom;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::ops::RangeInclusive;
use std::sync::Arc;
use storage::ConsensusStore;
use test_utils::mock_certificate_with_rand;
use test_utils::CommitteeFixture;
#[allow(unused_imports)]
use tokio::sync::mpsc::channel;
use types::CertificateAPI;
use types::HeaderAPI;
use types::Round;
use types::{Certificate, CertificateDigest};

#[derive(Copy, Clone, Debug)]
pub struct FailureModes {
    // The probability of having failures per round. As a failure is defined a node that does not produce
    // a certificate for a round (because is crashed, temporary failure or has just been slow). The failures should
    // be <=f , otherwise no DAG could be created. The provided number gives the probability of having
    // failures up to f. Ex for input `failures_probability = 0.2` it means we'll have 20% chance of
    // having failures up to 33% of the nodes.
    pub nodes_failure_probability: f64,

    // The percentage of slow nodes we want to introduce to our sample. Basically a slow node is one
    // that might be able to produce certificates, but these certificates never get referenced by others.
    // Consequently when those nodes are leaders they might also not get enough support - or no support at all.
    // For example, a value of 0.2 means that we want up to 20% of our nodes to behave as slow nodes.
    pub slow_nodes_percentage: f64,

    // The probability of failing to include a slow node certificate to from the certificates of next
    // round. For example a value of 0.1 means that 10% of the time fail get referenced by the
    // certificates of the next round.
    pub slow_nodes_failure_probability: f64,

    // The minimum committee size to apply the failure modes. If None then the failure mode will be
    // applied to any committee size. If Some is given and the committee size is smaller than this
    // number then the failure mode will be skipped.
    pub minimum_committee_size: Option<usize>,
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

#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn bullshark_randomised_tests() {
    // Configuration regarding the randomized tests. The tests will run for different values
    // on the below parameters to increase the different cases we can generate.

    // gc_depth to be used
    const GC_DEPTH: [Round; 3] = [6, 7, 10];
    // A the committee size values to be used
    const COMMITTEE_SIZE: [usize; 3] = [4, 7, 10];
    // Rounds for which we will create DAGs
    const DAG_ROUNDS: [Round; 6] = [6, 7, 8, 10, 12, 15];
    // The number of different execution plans to be created and tested against for every generated DAG
    const EXECUTION_PLANS: u64 = 500;
    // The number of DAGs that should be generated and tested against for every set of properties.
    const DAGS_PER_SETUP: u64 = 400;
    // DAGs will be created for these failure modes
    let failure_modes: Vec<FailureModes> = vec![
        // Some failures
        // TODO: re-enable once we do have parallel testing - now it worth testing the most severe
        // edge cases
        FailureModes {
            nodes_failure_probability: 0.10,     // 10%
            slow_nodes_percentage: 0.10,         // 10%
            slow_nodes_failure_probability: 0.3, // 30%
            minimum_committee_size: Some(7), // no reason to test this failure mode for smaller committee size, as we'll end up to similar
                                             // failures as the "severe failures" section
        },
        // Severe failures
        FailureModes {
            nodes_failure_probability: 0.0,      // 0%
            slow_nodes_percentage: 0.33,         // 33%
            slow_nodes_failure_probability: 0.7, // 70%
            minimum_committee_size: None,
        },
    ];

    let mut test_execution_list = FuturesUnordered::new();
    let (tx, mut rx) = channel(1000);

    #[derive(Debug)]
    struct TestData {
        dag_rounds: Round,
        gc_depth: Round,
        run_id: u64,
        committee_size: usize,
        mode: FailureModes,
    }

    tokio::spawn(async move {
        let mut run_id = 0;
        for committee_size in COMMITTEE_SIZE {
            for gc_depth in GC_DEPTH {
                for dag_rounds in DAG_ROUNDS {
                    for _ in 0..DAGS_PER_SETUP {
                        for mode in &failure_modes {
                            if mode.minimum_committee_size.unwrap_or_default() > committee_size {
                                continue;
                            }

                            // we want to skip this test as gc_depth will never be enforced
                            if gc_depth > dag_rounds {
                                continue;
                            }

                            run_id += 1;

                            tx.send(TestData {
                                dag_rounds,
                                gc_depth,
                                run_id,
                                committee_size,
                                mode: *mode,
                            })
                            .await
                            .unwrap();
                        }
                    }
                }
            }
        }
    });

    // Create a single store to be re-used across Bullshark instances to avoid hitting
    // a "too many files open" issue.
    let store = make_consensus_store(&test_utils::temp_dir());

    // Run the actual tests via separate tasks
    loop {
        tokio::select! {
            Some(data) = rx.recv(), if test_execution_list.len() < 20 => {
                let TestData{
                    dag_rounds,
                    gc_depth,
                    run_id,
                    committee_size,
                    mode
                } = data;

                let consensus_store = store.clone();

                let handle = tokio::spawn(async move {
                    // Create a randomized DAG
                    let (certificates, committee) =
                        generate_randomised_dag(committee_size, dag_rounds, run_id, mode);

                    // Now provide the DAG to create execution plans, run them via consensus
                    // and compare output against each other to ensure they are the same.
                    generate_and_run_execution_plans(
                        certificates,
                        EXECUTION_PLANS,
                        committee,
                        gc_depth,
                        dag_rounds,
                        run_id,
                        mode,
                        consensus_store
                    );
                });

                test_execution_list.push(handle);
            },
            Some(result) = test_execution_list.next() => {

                result.unwrap();
            },
            else => {
                break;
            }
        }
    }
}

/// Ensures that the methods to generate the DAGs and the execution plans are random but can be
/// reproduced by providing the same seed number - so practically they behave deterministically.
/// If that test breaks then we have no reassurance that we can reproduce the tests in case of
/// failure.
#[test]
fn test_determinism() {
    let committee_size = 4;
    let number_of_rounds = 2;
    let failure_modes = FailureModes {
        nodes_failure_probability: 0.0,
        slow_nodes_percentage: 0.33,
        slow_nodes_failure_probability: 0.5,
        minimum_committee_size: None,
    };

    for seed in 0..=10 {
        // Compare the creation of DAG & committee
        let (dag_1, committee_1) =
            generate_randomised_dag(committee_size, number_of_rounds, seed, failure_modes);
        let (dag_2, committee_2) =
            generate_randomised_dag(committee_size, number_of_rounds, seed, failure_modes);

        assert_eq!(committee_1, committee_2);
        assert_eq!(dag_1, dag_2);

        // Compare the creation of execution plan based on the provided DAG
        let execution_plan_1 = create_execution_plan(dag_1.clone(), seed);
        let execution_plan_2 = create_execution_plan(dag_1, seed);

        assert_eq!(execution_plan_1.certificates, execution_plan_2.certificates);
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

    // Pick the slow nodes - ensure we don't have more than 33% of slow nodes
    assert!(modes.slow_nodes_percentage <= 0.33, "Slow nodes can't be more than 33% of total nodes - otherwise we'll basically simulate a consensus stall");

    let mut authorities: Vec<Authority> = committee.authorities().cloned().collect();

    // Now shuffle authorities and pick the slow nodes, if should exist
    authorities.shuffle(&mut rand);

    // Step 1 - determine the slow nodes , assuming those should exist
    let slow_nodes: Vec<(Authority, f64)> = {
        let stake_of_slow_nodes =
            (committee.total_stake() as f64 * modes.slow_nodes_percentage) as Stake;
        let stake_of_slow_nodes = stake_of_slow_nodes.min(committee.validity_threshold() - 1);
        let mut total_stake = 0;

        authorities
            .iter()
            .take_while(|a| {
                total_stake += a.stake();
                total_stake <= stake_of_slow_nodes
            })
            .map(|k| (k.clone(), 1.0 - modes.slow_nodes_failure_probability))
            .collect()
    };

    println!(
        "Slow nodes: {:?}",
        slow_nodes
            .iter()
            .map(|(a, _)| a.id())
            .collect::<Vec<AuthorityIdentifier>>()
    );

    let mut certificates = VecDeque::new();
    let mut parents = initial_parents;
    let mut next_parents = Vec::new();
    let mut certificate_digests: HashSet<CertificateDigest> =
        parents.iter().map(|c| c.digest()).collect();

    for round in range {
        next_parents.clear();

        let mut total_round_stake = 0;
        let mut total_failures = 0;

        // shuffle authorities to introduce extra randomness
        authorities.shuffle(&mut rand);

        for authority in authorities.iter() {
            let current_parents = parents.clone();

            // Step 2 -- introduce failures (assuming those are enabled)
            // We disable the failure probability if we have already reached the maximum number
            // of allowed failures (f)
            let should_fail = if committee.reached_validity(total_failures + 1) {
                false
            } else {
                let b = Bernoulli::new(modes.nodes_failure_probability).unwrap();
                b.sample(&mut rand)
            };

            if should_fail {
                total_failures += 1;
                continue;
            }

            // Step 3 -- to form the certificate we need to figure out the certificate's parents
            // we are going to pick taking into account the slow nodes.
            let ids: Vec<(AuthorityIdentifier, f64)> = slow_nodes
                .iter()
                .map(|(a, inclusion_probability)| (a.id(), *inclusion_probability))
                .collect();

            let mut parent_digests: BTreeSet<CertificateDigest> =
                test_utils::this_cert_parents_with_slow_nodes(
                    &authority.id(),
                    current_parents.clone(),
                    ids.as_slice(),
                    &mut rand,
                    committee,
                );

            // We want to ensure that we always refer to "our" certificate of the previous round -
            // assuming that exist, so we can re-add it later.
            let my_parent_digest = if let Some(my_previous_round) = current_parents
                .iter()
                .find(|c| c.origin() == authority.id())
            {
                parent_digests.remove(&my_previous_round.digest());
                Some(my_previous_round.digest())
            } else {
                None
            };

            let mut parent_digests: Vec<CertificateDigest> = parent_digests.into_iter().collect();

            // Step 4 -- references to previous round
            // Now from the rest of current_parents, pick a random number - uniform - to how many
            // should create references to. It should strictly be between [2f+1..3f+1].
            let num_of_parents_to_pick =
                rand.gen_range(committee.quorum_threshold()..=committee.total_stake());

            // shuffle the parents
            parent_digests.shuffle(&mut rand);

            // now keep only the num_of_parents_to_pick
            let mut parents_digests: Vec<CertificateDigest> = parent_digests
                .into_iter()
                .take(num_of_parents_to_pick as usize)
                .collect();

            // Now swap one if necessary with our own
            if let Some(my_parent_digest) = my_parent_digest {
                // remove one only if we have at least a quorum
                if parents_digests.len() >= committee.quorum_threshold() as usize {
                    parents_digests.pop();
                }
                parents_digests.insert(0, my_parent_digest);
            }

            assert!(
                parents_digests.len() >= committee.quorum_threshold() as usize,
                "Failed on seed {}. At least 2f+1 parents are needed.",
                seed
            );

            let parents_digests: BTreeSet<CertificateDigest> =
                parents_digests.into_iter().collect();

            // Now create the certificate with the provided parents
            let (_, certificate) = mock_certificate_with_rand(
                committee,
                authority.id(),
                round,
                parents_digests.clone(),
                &mut rand,
            );

            // group certificates by round for easy access
            certificate_digests.insert(certificate.digest());

            certificates.push_back(certificate.clone());
            next_parents.push(certificate);

            // update the total round stake
            total_round_stake += authority.stake();
        }
        parents = next_parents.clone();

        // Sanity checks
        // Ensure total stake of the round provides strong quorum
        assert!(
            committee.reached_quorum(total_round_stake),
            "Failed on seed {}. Strong quorum is needed per round to ensure DAG advance.",
            seed
        );

        // Ensure each certificate's parents exist from previous processing
        parents
            .iter()
            .flat_map(|c| c.header().parents())
            .for_each(|digest| {
                assert!(
                    certificate_digests.contains(digest),
                    "Failed on seed {}. Certificate with digest {} should be found in processed certificates.",
                    seed,
                    digest
                );
            });
    }

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
    modes: FailureModes,
    store: Arc<ConsensusStore>,
) {
    println!(
        "Running execution plans for run_id {} for rounds={}, committee={}, gc_depth={}, modes={:?}",
        run_id,
        dag_rounds,
        committee.size(),
        gc_depth,
        modes
    );

    let mut executed_plans = HashSet::new();
    let mut committed_certificates = Vec::new();

    for i in 0..test_iterations {
        let seed = (i + 1) + run_id;

        let plan = create_execution_plan(original_certificates.clone(), seed);

        let hash = plan.hash();
        if !executed_plans.insert(hash) {
            println!("Skipping plan with seed {}, same executed already", seed);
            continue;
        }

        // Now create a new Bullshark engine
        let metrics = Arc::new(ConsensusMetrics::new(&Registry::new()));
        let mut state = ConsensusState::new(metrics.clone(), gc_depth);
        let mut bullshark = Bullshark::new(
            committee.clone(),
            store.clone(),
            metrics.clone(),
            NUM_SUB_DAGS_PER_SCHEDULE,
        );

        let mut inserted_certificates = HashSet::new();

        let mut plan_committed_certificates = Vec::new();
        for c in plan.certificates {
            // A sanity check that we indeed attempt to send to Bullshark a certificate
            // whose parents have already been inserted.
            if c.round() > 1 {
                for parent in c.header().parents() {
                    assert!(inserted_certificates.contains(parent));
                }
            }
            inserted_certificates.insert(c.digest());

            // Now commit one by one the certificates and gather the results
            let (_outcome, committed_sub_dags) =
                bullshark.process_certificate(&mut state, c).unwrap();
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
                "Fork detected in plans for run_id={}, seed={}, rounds={}, committee={}, gc_depth={}, modes={:?}",
                run_id,
                seed,
                dag_rounds,
                committee.size(),
                gc_depth,
                modes
            );
        }
    }

    println!("Successfully run {}", run_id);
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
            for parent in certificate.header().parents() {
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
