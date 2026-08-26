// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Bounded language-semantics checks for regex paths.
//!
//! The structural property tests exercise algebraic identities. These tests instead interpret
//! regexes over a small alphabet. They check extension soundness and precision, exact prefix
//! quotients, and the relation factoring performed while extending a graph.
//!
//! `matches_regex` is the independent model: a regex is either one exact path or a fixed prefix
//! followed by any labels. Proptest chooses the regexes, while each property exhaustively checks
//! every concrete path within the bounds below.

use crate::{
    collections::{Graph, Path},
    meter::DummyMeter,
    references::Ref,
    regex::{Extension, Regex},
};
use proptest::prelude::*;
use std::collections::BTreeSet;

const ALPHABET: [char; 2] = ['a', 'b'];
// Two distinct labels expose ordering and mismatch bugs. These bounds keep exhaustive enumeration
// cheap while covering paths longer than any generated fixed prefix.
const MAX_PATH_LEN: usize = 5;
const MAX_REGEX_LABELS: usize = 3;

// Enumerate every concrete path over ALPHABET up to max_len.
fn universe_up_to(max_len: usize) -> Vec<Vec<char>> {
    let mut paths = vec![vec![]];
    let mut frontier = vec![vec![]];
    for _ in 0..max_len {
        let mut next = vec![];
        for path in &frontier {
            for label in ALPHABET {
                let mut extended = path.clone();
                extended.push(label);
                paths.push(extended.clone());
                next.push(extended);
            }
        }
        frontier = next;
    }
    paths
}

// The concrete paths checked against each generated regex.
fn universe() -> Vec<Vec<char>> {
    universe_up_to(MAX_PATH_LEN)
}

fn matches_regex(regex: &Regex<char>, path: &[char]) -> bool {
    if regex.ends_in_dot_star {
        // Check that regex.labels is a prefix of path.
        path.starts_with(&regex.labels)
    } else {
        path == regex.labels
    }
}

fn matches_extension(extension: &Extension<char>, path: &[char]) -> bool {
    match extension {
        Extension::Epsilon => path.is_empty(),
        Extension::Label(label) => path == [*label],
        Extension::DotStar => true,
    }
}

fn concatenate(prefix: &[char], suffix: &[char]) -> Vec<char> {
    prefix.iter().chain(suffix).copied().collect()
}

fn matches_any(regexes: &[Regex<char>], path: &[char]) -> bool {
    regexes.iter().any(|regex| matches_regex(regex, path))
}

// Generate every supported Regex shape with a bounded fixed prefix.
fn arb_regex() -> impl Strategy<Value = Regex<char>> {
    (
        prop::collection::vec(
            prop::sample::select(ALPHABET.as_slice()),
            0..=MAX_REGEX_LABELS,
        ),
        any::<bool>(),
    )
        .prop_map(|(labels, ends_in_dot_star)| Regex {
            labels,
            ends_in_dot_star,
        })
}

// Generate epsilon, a single label, or dot-star.
fn arb_extension() -> impl Strategy<Value = Extension<char>> {
    prop_oneof![
        Just(Extension::Epsilon),
        prop::sample::select(ALPHABET.as_slice()).prop_map(Extension::Label),
        Just(Extension::DotStar),
    ]
}

// Convert an internal Regex into the corresponding query API Path.
fn as_path(regex: &Regex<char>) -> Path<(), char> {
    Path {
        loc: (),
        labels: regex.labels.clone(),
        ends_in_dot_star: regex.ends_in_dot_star,
    }
}

// Apply an Extension through the matching public graph operation.
fn extend_graph(
    graph: &mut Graph<(), char>,
    sources: &BTreeSet<Ref>,
    extension: &Extension<char>,
    is_mut: bool,
) -> Ref {
    match extension {
        Extension::Epsilon => graph
            .extend_by_epsilon((), sources.iter().copied(), is_mut, &mut DummyMeter)
            .unwrap(),
        Extension::Label(label) => graph
            .extend_by_label((), sources.iter().copied(), is_mut, *label, &mut DummyMeter)
            .unwrap(),
        Extension::DotStar => graph
            .extend_by_dot_star_for_call((), sources, vec![is_mut], &mut DummyMeter)
            .unwrap()[0],
    }
}

// Capture every graph edge as a union of regex languages. Self-epsilon edges are added explicitly
// because the public query API intentionally omits them.
fn relation_snapshot(graph: &Graph<(), char>, refs: &[Ref]) -> Vec<Vec<Vec<Regex<char>>>> {
    let mut relations = vec![vec![vec![]; refs.len()]; refs.len()];
    for (source_idx, source) in refs.iter().copied().enumerate() {
        relations[source_idx][source_idx].push(Regex::epsilon());
        let borrowed = graph.borrowed_by(source, &mut DummyMeter).unwrap();
        for (target_idx, target) in refs.iter().copied().enumerate() {
            let Some(paths) = borrowed.get(&target) else {
                continue;
            };
            relations[source_idx][target_idx].extend(paths.iter().map(|path| Regex {
                labels: path.labels.clone(),
                ends_in_dot_star: path.ends_in_dot_star,
            }));
        }
    }
    relations
}

// Check whether a captured source-to-target relation contains path.
fn relation_matches(
    relations: &[Vec<Vec<Regex<char>>>],
    source: usize,
    target: usize,
    path: &[char],
) -> bool {
    matches_any(&relations[source][target], path)
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 512,
        ..ProptestConfig::default()
    })]

    // Soundness property: abstract extension may over-approximate a concatenation, but it must
    // never omit a concrete path from it.
    #[test]
    fn extend_covers_bounded_concatenation(
        regex in arb_regex(),
        extension in arb_extension(),
    ) {
        let result = regex.clone().extend(&extension);
        for path in universe() {
            let in_concatenation = (0..=path.len()).any(|split| {
                matches_regex(&regex, &path[..split])
                    && matches_extension(&extension, &path[split..])
            });
            prop_assert!(
                !in_concatenation || matches_regex(&result, &path),
                "{:?}.extend({:?}) = {:?} dropped {:?}",
                regex,
                extension,
                result,
                path,
            );
        }
    }

    // Extension is exact except when a label follows dot-star. That case intentionally drops the
    // trailing label because the current regex representation cannot express `prefix.*label`.
    #[test]
    fn extend_matches_bounded_concatenation_when_representable(
        regex in arb_regex(),
        extension in arb_extension(),
    ) {
        let loses_precision =
            regex.ends_in_dot_star && matches!(extension, Extension::Label(_));
        // An early return rather than `prop_assume!`: this case is rejected roughly one time in
        // six, so with a raised case count the reject accounting aborts the test instead of
        // giving it more coverage.
        if loses_precision {
            return Ok(());
        }

        let result = regex.clone().extend(&extension);
        for path in universe() {
            let in_concatenation = (0..=path.len()).any(|split| {
                matches_regex(&regex, &path[..split])
                    && matches_extension(&extension, &path[split..])
            });
            prop_assert_eq!(
                matches_regex(&result, &path),
                in_concatenation,
                "{:?}.extend({:?}) = {:?} disagreed on {:?}",
                regex,
                extension,
                result,
                path,
            );
        }
    }

    // A suffix is in regex.remove_prefix(extension) exactly when some path matched by `extension`
    // can be prepended to that suffix to produce a path matched by `regex`.
    #[test]
    fn regex_remove_prefix_matches_bounded_left_quotient(
        regex in arb_regex(),
        extension in arb_extension(),
    ) {
        let results = regex.remove_prefix(&extension);
        let prefixes = universe_up_to(MAX_REGEX_LABELS);
        for suffix in universe() {
            let in_quotient = prefixes.iter().any(|prefix| {
                matches_extension(&extension, prefix)
                    && matches_regex(&regex, &concatenate(prefix, &suffix))
            });
            prop_assert_eq!(
                matches_any(&results, &suffix),
                in_quotient,
                "{:?}.remove_prefix({:?}) = {:?} disagreed on {:?}",
                regex,
                extension,
                results,
                suffix,
            );
        }
    }

    // Check the same left-quotient definition with the operand restrictions reversed.
    #[test]
    fn extension_remove_prefix_matches_bounded_left_quotient(
        extension in arb_extension(),
        prefix_regex in arb_regex(),
    ) {
        let results = extension.remove_prefix(&prefix_regex);
        let prefixes = universe_up_to(MAX_REGEX_LABELS);
        for suffix in universe() {
            let in_quotient = prefixes.iter().any(|prefix| {
                matches_regex(&prefix_regex, prefix)
                    && matches_extension(&extension, &concatenate(prefix, &suffix))
            });
            prop_assert_eq!(
                matches_any(&results, &suffix),
                in_quotient,
                "{:?}.remove_prefix({:?}) = {:?} disagreed on {:?}",
                extension,
                prefix_regex,
                results,
                suffix,
            );
        }
    }

    // Build a valid graph history, extend several selected references, and recompute the required
    // new relationships from the concrete languages present before the extension.
    #[test]
    fn graph_extension_covers_relation_factoring(
        initial_mutabilities in any::<(bool, bool)>(),
        history in prop::collection::vec(
            (arb_extension(), any::<u8>(), any::<bool>()),
            0..=4,
        ),
        source_choices in prop::collection::vec(any::<u8>(), 1..=3),
        extension in arb_extension(),
        result_is_mut in any::<bool>(),
    ) {
        let (mut graph, initial_refs) = Graph::<(), char>::new(
            8,
            [
                (0, (), initial_mutabilities.0),
                (1, (), initial_mutabilities.1),
            ],
        )
        .unwrap();
        let mut refs = vec![initial_refs[&0], initial_refs[&1]];

        for (history_extension, source_choice, is_mut) in history {
            let source = refs[source_choice as usize % refs.len()];
            let new_ref = extend_graph(
                &mut graph,
                &BTreeSet::from([source]),
                &history_extension,
                is_mut,
            );
            refs.push(new_ref);
        }

        let selected_source_indices = source_choices
            .into_iter()
            .map(|choice| choice as usize % refs.len())
            .collect::<BTreeSet<_>>();
        let selected_sources = selected_source_indices
            .iter()
            .map(|&idx| refs[idx])
            .collect::<BTreeSet<_>>();
        let effective_source_indices = selected_source_indices
            .iter()
            .copied()
            .filter(|&idx| {
                !matches!(extension, Extension::DotStar)
                    || !result_is_mut
                    || graph.is_mutable(refs[idx]).unwrap()
            })
            .collect::<Vec<_>>();
        let before = relation_snapshot(&graph, &refs);

        let new_ref =
            extend_graph(&mut graph, &selected_sources, &extension, result_is_mut);
        let mut refs_after = refs.clone();
        refs_after.push(new_ref);
        let after = relation_snapshot(&graph, &refs_after);
        let new_idx = refs.len();
        let witness_paths = universe();

        for existing_idx in 0..refs.len() {
            for path in &witness_paths {
                // new --path--> existing when extension·path was already a path from a source.
                let new_to_existing = effective_source_indices.iter().any(|&source_idx| {
                    witness_paths.iter().any(|prefix| {
                        matches_extension(&extension, prefix)
                            && relation_matches(
                                &before,
                                source_idx,
                                existing_idx,
                                &concatenate(prefix, path),
                            )
                    })
                });
                prop_assert!(
                    !new_to_existing
                        || relation_matches(&after, new_idx, existing_idx, path),
                    "new relation dropped: sources={:?}, extension={:?}, new -> {}, path={:?}",
                    effective_source_indices,
                    extension,
                    existing_idx,
                    path,
                );

                // existing --path--> new either extends an existing path into a source, or factors
                // a source-to-existing path out of the new extension.
                let existing_to_new = effective_source_indices.iter().any(|&source_idx| {
                    let extends_incoming = (0..=path.len()).any(|split| {
                        relation_matches(
                            &before,
                            existing_idx,
                            source_idx,
                            &path[..split],
                        ) && matches_extension(&extension, &path[split..])
                    });
                    let factors_outgoing = witness_paths.iter().any(|prefix| {
                        relation_matches(
                            &before,
                            source_idx,
                            existing_idx,
                            prefix,
                        ) && matches_extension(
                            &extension,
                            &concatenate(prefix, path),
                        )
                    });
                    extends_incoming || factors_outgoing
                });
                prop_assert!(
                    !existing_to_new
                        || relation_matches(&after, existing_idx, new_idx, path),
                    "new relation dropped: sources={:?}, extension={:?}, {} -> new, path={:?}",
                    effective_source_indices,
                    extension,
                    existing_idx,
                    path,
                );
            }
        }
    }

    // These predicates drive writability and local-borrow checks in the bytecode verifier.
    #[test]
    fn path_predicates_match_bounded_language(regex in arb_regex()) {
        let path = as_path(&regex);
        let paths = universe();
        let matching = paths
            .iter()
            .filter(|candidate| matches_regex(&regex, candidate))
            .collect::<Vec<_>>();

        let is_epsilon = matching.len() == 1 && matching[0].is_empty();
        prop_assert_eq!(path.is_epsilon(), is_epsilon);

        let is_dot_star = matching.len() == paths.len();
        prop_assert_eq!(path.is_dot_star(), is_dot_star);

        for label in ALPHABET {
            let is_label =
                matching.len() == 1 && matching[0].as_slice() == [label];
            prop_assert_eq!(path.is_label(&label), is_label);

            let starts_with = matching
                .iter()
                .any(|candidate| candidate.first() == Some(&label));
            prop_assert_eq!(path.starts_with(&label), starts_with);
        }
    }
}

// Calls treat mutable and immutable results differently: mutable results only extend mutable
// arguments, while immutable results extend every argument and may alias each other.
#[test]
fn call_results_obey_mutability_partition() {
    let (mut graph, sources) = Graph::<(), char>::new(6, [(0, (), true), (1, (), false)]).unwrap();
    let mutable_source = sources[&0];
    let immutable_source = sources[&1];
    let results = graph
        .extend_by_dot_star_for_call(
            (),
            &BTreeSet::from([mutable_source, immutable_source]),
            vec![true, true, false, false],
            &mut DummyMeter,
        )
        .unwrap();
    let refs = [
        mutable_source,
        immutable_source,
        results[0],
        results[1],
        results[2],
        results[3],
    ];
    let relations = relation_snapshot(&graph, &refs);

    for mutable_result in [2, 3] {
        assert!(relation_matches(&relations, 0, mutable_result, &['a']));
        assert!(relations[1][mutable_result].is_empty());
    }

    for immutable_result in [4, 5] {
        for source in [0, 1] {
            assert!(relation_matches(
                &relations,
                source,
                immutable_result,
                &['a']
            ));
        }
    }

    assert!(relation_matches(&relations, 4, 5, &['a']));
    assert!(relation_matches(&relations, 5, 4, &['a']));

    for mutable_result in [2, 3] {
        for other_result in [2, 3, 4, 5] {
            if mutable_result != other_result {
                assert!(relations[mutable_result][other_result].is_empty());
                assert!(relations[other_result][mutable_result].is_empty());
            }
        }
    }
}
