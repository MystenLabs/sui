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
    (prop::collection::vec(arb_label(), 0..5), any::<bool>()).prop_map(
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
    // Dot-Star is an absorbing element under extension
    // -------------------------------------------------------------------------
    #[test]
    fn dotstar_extend_absords_right(r in arb_regex(), ext in arb_extension()) {
        let r1 = r.clone().extend(&Extension::DotStar);
        let r2 = r1.clone().extend(&ext);
        prop_assert!(r1.ends_in_dot_star);
        prop_assert_eq!(r1, r2);
    }

    // -------------------------------------------------------------------------
    // 3. Dot-Start extension is idempotent
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
    //
    // For any regex r, removing any label from a dot-star still yields [".*"].
    //
    #[test]
    fn dotstar_remove_prefix_absorb_left(e in arb_extension()) {
        let ds = Regex::dot_star();
        let removed = ds.clone().remove_prefix(&e);
        prop_assert_eq!(removed, vec![Regex::dot_star()]);
    }

    // -----------------------------------------------------------------------------
    // Dot-star prefix removal preserves dot-star termination
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
    // Abstract size is monotonic under extension
    // -------------------------------------------------------------------------
    #[test]
    fn abstract_size_monotone(r in arb_regex(), ext in arb_extension()) {
        let before = r.abstract_size();
        let after = r.clone().extend(&ext).abstract_size();
        prop_assert!(after >= before);
    }

    // -------------------------------------------------------------------------
    // Label extension increases size unless Dot-Star was set
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
    fn remove_prefix_duplicates(r in arb_regex(), ext in arb_extension()) {
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
