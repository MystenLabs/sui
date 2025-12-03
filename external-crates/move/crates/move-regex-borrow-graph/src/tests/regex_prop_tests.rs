// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::regex::{Extension, Regex};
use proptest::prelude::*;

/// Generate arbitrary labels (for now use simple `char` or `u8`)
fn arb_label() -> impl Strategy<Value = char> {
    // Avoid control characters for cleaner display/debugging
    prop::char::range('a', 'z')
}

/// Strategy for generating arbitrary Regex<Lbl>
fn arb_regex() -> impl Strategy<Value = Regex<char>> {
    (prop::collection::vec(arb_label(), 0..7), any::<bool>()).prop_map(
        |(labels, ends_in_dot_star)| Regex {
            labels,
            ends_in_dot_star,
        },
    )
}

/// Strategy for generating arbitrary Extension<Lbl>
fn arb_extension() -> impl Strategy<Value = Extension<char>> {
    prop_oneof![
        Just(Extension::Epsilon),
        arb_label().prop_map(Extension::Label),
        Just(Extension::DotStar),
    ]
}

proptest! {
    // -------------------------------------------------------------------------
    // Epsilon is a right identity under extend
    // -------------------------------------------------------------------------
    #[test]
    fn epsilon_extend_identity_right(r in arb_regex()) {
        let eps = Extension::Epsilon;
        prop_assert_eq!(r.clone().extend(&eps), r);
    }

    // -------------------------------------------------------------------------
    // Epsilon is a left identity under extend
    // -------------------------------------------------------------------------
    #[test]
    fn epsilon_extend_identity_left(l in arb_label()) {
        let empty = Regex::epsilon();
        let extension = Extension::Label(l);
        let single = empty.clone().extend(&extension);
        prop_assert!(single.labels.len() == 1);
        prop_assert!(single.labels[0] == l);
    }

    // -------------------------------------------------------------------------
    // Epsilon is a left identity under prefix removal
    // -------------------------------------------------------------------------
    #[test]
    fn epsilon_prefix_identity_left(r in arb_extension()) {
        let empty = Regex::epsilon();
        let removed = empty.clone().remove_prefix(&r);
        match r {
            Extension::Epsilon => prop_assert_eq!(removed, vec![empty]),
            Extension::DotStar => prop_assert_eq!(removed, vec![empty]),
            _ => prop_assert_eq!(removed.len(), 0),
        }
    }

    // -------------------------------------------------------------------------
    // Epsilon is a right identity under prefix removal
    // -------------------------------------------------------------------------
    #[test]
    fn epsilon_prefix_identity_right(r in arb_regex()) {
        let removed = r.clone().remove_prefix(&Extension::Epsilon);
        prop_assert_eq!(vec![r], removed);
    }

    // -------------------------------------------------------------------------
    // Epsilon extension is idempotent and has no observable effect
    // -------------------------------------------------------------------------
    #[test]
    fn epsilon_idempotent(r in arb_regex()) {
        let eps = Extension::Epsilon;
        let r1 = r.clone().extend(&eps);
        let r2 = r1.clone().extend(&eps);
        prop_assert_eq!(&r, &r1);
        prop_assert_eq!(r1, r2);
    }

    // -------------------------------------------------------------------------
    // Dot-star is an absorbing element under extension
    // -------------------------------------------------------------------------
    #[test]
    fn dotstar_extend_absorbs_right(r in arb_regex(), ext in arb_extension()) {
        let r1 = r.clone().extend(&Extension::DotStar);
        let r2 = r1.clone().extend(&ext);
        prop_assert!(r1.ends_in_dot_star);
        prop_assert_eq!(r1, r2);
    }

    // -------------------------------------------------------------------------
    // Dot-star extension is idempotent
    // -------------------------------------------------------------------------
    #[test]
    fn dotstar_idempotent(r in arb_regex()) {
        let r1 = r.clone().extend(&Extension::DotStar);
        let r2 = r1.clone().extend(&Extension::DotStar);
        prop_assert_eq!(r1, r2);
    }

    // -------------------------------------------------------------------------
    // Dot-star removal always returns a non-empty list (closure property)
    // -------------------------------------------------------------------------
    #[test]
    fn remove_prefix_dotstar_nonempty(r in arb_regex()) {
        let res = r.remove_prefix(&Extension::DotStar);
        prop_assert!(!res.is_empty());
    }

    // -----------------------------------------------------------------------------
    // Dot-star absorption under prefix removal
    // -----------------------------------------------------------------------------
    #[test]
    fn dotstar_remove_prefix_absorb_left(e in arb_extension()) {
        let ds = Regex::dot_star();
        let removed = ds.clone().remove_prefix(&e);
        prop_assert_eq!(removed, vec![Regex::dot_star()]);
    }

    // -----------------------------------------------------------------------------
    // Dot-star prefix removal preserves dot-star termination
    // -----------------------------------------------------------------------------
    #[test]
    fn dotstar_prefix_preserves_flag(l in arb_label()) {
        let r = Regex { labels: vec![l], ends_in_dot_star: true };
        let removed = r.remove_prefix(&Extension::Label(l));
        prop_assert!(!removed.is_empty());
        for rr in removed {
            prop_assert!(rr.ends_in_dot_star);
        }
    }

    // -------------------------------------------------------------------------
    // Dot-star is stable under arbitrary sequences of extensions and removals
    // -------------------------------------------------------------------------
    #[test]
    fn dotstar_stability(
        exts in prop::collection::vec(arb_extension(), 0..5),
    ) {
        let mut r = Regex::dot_star();
        for ext in &exts {
            r = r.extend(ext);
            let removed = r.remove_prefix(ext); // Remove randomly
            for rr in removed {
                prop_assert!(rr.ends_in_dot_star);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Abstract size is monotonic under extension
    // -------------------------------------------------------------------------
    #[test]
    fn abstract_size_monotone(r in arb_regex(), ext in arb_extension()) {
        let before = r.abstract_size();
        let after = r.clone().extend(&ext).abstract_size();
        match ext {
            Extension::Epsilon => prop_assert_eq!(after, before),
            Extension::DotStar => prop_assert!(after == before || after == before + 1),
            Extension::Label(_) if r.ends_in_dot_star => prop_assert!(after == before),
            Extension::Label(_) => prop_assert!(after == before + 1),
        };
    }

    // -------------------------------------------------------------------------
    // Label extension increases size unless Dot-star was set
    // -------------------------------------------------------------------------
    #[test]
    fn label_extension_increases_length(r in arb_regex(), lbl in arb_label()) {
        let base_len = r.labels.len();
        let was_dotstar = r.ends_in_dot_star;
        let r2 = r.clone().extend(&Extension::Label(lbl));
        let new_len = r2.labels.len();

        if was_dotstar {
            prop_assert_eq!(new_len, base_len);
        } else {
            prop_assert_eq!(new_len, base_len + 1);
        }
    }

    // -------------------------------------------------------------------------
    // The round trip via Extension::into_regex reconstructs equivalent Regex
    // -------------------------------------------------------------------------
    #[test]
    fn into_regex_roundtrip(ext in arb_extension()) {
        let regex = ext.clone().into_regex();
        // If we convert back to extension, we can only check partial equality:
        // epsilon and dot_star are distinct cases, others are label-wrapped
        match ext {
            Extension::Epsilon => prop_assert!(regex.is_epsilon()),
            Extension::DotStar => prop_assert!(regex.ends_in_dot_star),
            Extension::Label(lbl) => {
                let (labels, dot_star) = regex.query_api_path();
                prop_assert_eq!(labels, vec![lbl]);
                prop_assert!(!dot_star);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Removing a prefix produces distinct elements
    // -------------------------------------------------------------------------
    #[test]
    fn remove_prefix_unique(r in arb_regex(), ext in arb_extension()) {
        let result = r.remove_prefix(&ext);
        let mut unique = result.clone();
        unique.dedup();
        prop_assert_eq!(unique.len(), result.len());
    }

    // -------------------------------------------------------------------------
    /// Label extension then removing prefixes preserves extension
    // -------------------------------------------------------------------------
    #[test]
    fn extend_then_remove_prefix_recovers_suffix(
        base_labels in prop::collection::vec(arb_label(), 0..4),
        suffix_labels in prop::collection::vec(arb_label(), 0..4),
    ) {
        // Construct base and suffix regexes
        let base = Regex { labels: base_labels.clone(), ends_in_dot_star: false };
        let mut extended = base.clone();

        // Sequentially extend by each label in the suffix
        for lbl in &suffix_labels {
            extended = extended.extend(&Extension::Label(*lbl));
        }

        // Remove the base prefix from the extended regex
        for label in &base_labels {
            let subresults = extended.remove_prefix(&Extension::Label(*label));
            prop_assert!(subresults.len() == 1);
            extended = subresults[0].clone();
        }

        // After removing each base label, the remaining regex labels should match the suffix
        let (rem_labels, _rem_dot_star) = extended.query_api_path();
        prop_assert_eq!(rem_labels, suffix_labels);
    }

    // -----------------------------------------------------------------------------
    // Dot-star prefix removal reproduces all possible entries
    // -----------------------------------------------------------------------------
    #[test]
    fn dotstar_remove_prefix_produces_all_entries(r in arb_regex()) {
        let removed = r.remove_prefix(&Extension::DotStar);

        // If the regex ends in dot-star, the only result is dot-star itself
        if r.ends_in_dot_star {
            prop_assert!(removed.contains(&Regex::dot_star()));
            prop_assert!(removed.len() == 1);
            return Ok(());
        }

        prop_assert!(removed.contains(&Regex::epsilon()));
        prop_assert_eq!(removed.len(), r.labels.len() + 1);
        for i in 0..r.labels.len() {
            let expected = Regex {
                labels: r.labels[i + 1..].to_vec(),
                ends_in_dot_star: r.ends_in_dot_star,
            };
            prop_assert!(removed.contains(&expected),
                "Expected to find {:?} in removed set {:?}", expected, removed);
        }
    }

    // -------------------------------------------------------------------------
    // Removing any prefix of R from (R + S) yields the corresponding suffix + S
    // -------------------------------------------------------------------------
    #[test]
    fn remove_partial_prefix_from_extended(
        base_labels in prop::collection::vec(arb_label(), 0..4),
        suffix_labels in prop::collection::vec(arb_label(), 0..3),
    ) {
        let base = Regex { labels: base_labels.clone(), ends_in_dot_star: false };
        let mut extended = base.clone();
        for lbl in &suffix_labels {
            extended = extended.extend(&Extension::Label(*lbl));
        }

        // For each possible prefix length k, remove that prefix
        for k in 0..=base_labels.len() {
            let prefix = &base_labels[..k];
            let mut current = extended.clone();
            for lbl in prefix {
                let subresults = current.remove_prefix(&Extension::Label(*lbl));
                if subresults.is_empty() {
                    return Ok(());
                }
                current = subresults[0].clone();
            }

            // Expect result to have labels = base[k..] + suffix
            let (rem_labels, _) = current.query_api_path();
            let expected = base_labels[k..].iter().chain(suffix_labels.iter()).cloned().collect::<Vec<_>>();
            prop_assert_eq!(rem_labels, expected);
        }
    }

    // -------------------------------------------------------------------------
    // Removing a prefix produces only suffixes (closure under path factoring)
    // -------------------------------------------------------------------------
    #[test]
    fn remove_prefix_closure(r in arb_regex(), ext in arb_extension()) {
        let res = r.remove_prefix(&ext);
        for q in res {
            // q.labels should be subset (suffix) of r.labels
            if !r.ends_in_dot_star {
                let is_suffix = r.labels.ends_with(&q.labels);
                prop_assert!(is_suffix || q.is_epsilon() || q.ends_in_dot_star,
                    "remove_prefix produced non-suffix {:?} from {:?}", q, r);
            }
        }
    }

    // -------------------------------------------------------------------------
    // Extending by label then removing epsilon keeps the result unchanged
    // -------------------------------------------------------------------------
    #[test]
    fn extend_then_remove_epsilon_no_effect(r in arb_regex(), lbl in arb_label()) {
        let ext = Extension::Label(lbl);
        let extended = r.clone().extend(&ext);
        let eps = Extension::Epsilon;
        let removed = extended.remove_prefix(&eps);
        prop_assert_eq!(removed, vec![extended]);
    }

    // -------------------------------------------------------------------------
    // Abstract size should not decrease through remove+extend sequence
    // -------------------------------------------------------------------------
    #[test]
    fn size_preserved_under_remove_then_extend(
        mut r in arb_regex(),
        lbl in arb_label(),
    ) {
        r.ends_in_dot_star = false; // Ensure size changes are observable
        let before = r.abstract_size();
        let removed = r.remove_prefix(&Extension::Label(lbl));
        for rr in &removed {
            let reextended = rr.clone().extend(&Extension::Label(lbl));
            prop_assert!(reextended.abstract_size() >= before);
        }
    }

    // -------------------------------------------------------------------------
    // Random walk over extend/remove ops never panics or invalidates regex
    // -------------------------------------------------------------------------
    #[test]
    fn random_walk_over_operations(
        ops in prop::collection::vec(arb_extension(), 1..50)
    ) {
        let mut r = Regex::epsilon();
        for op in &ops {
            r = r.extend(op);
            let removed = r.remove_prefix(op);
            for rr in removed {
                // Ensure valid regex (well-formedness)
                prop_assert!(rr.abstract_size() >= 1);
            }
        }
    }
}

// -------------------------------------------------------------------------------------------------
// Debug
// -------------------------------------------------------------------------------------------------

impl std::fmt::Debug for Extension<char> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Extension::Epsilon => write!(f, "ε"),
            Extension::DotStar => write!(f, ".*"),
            Extension::Label(lbl) => write!(f, "{}", lbl),
        }
    }
}
