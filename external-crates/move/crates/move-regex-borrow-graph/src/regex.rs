// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{Debug, Display};

/// An edge in the graph contains a set of these simple regular expressions. For now, this is
/// just a (potentially empty) list of labels, and a flag indicating if the last label is a dot
/// star. This is overly constrained to the current limitations in Move. When we add support for
/// recursive types, we will need a more general regular expression. Particularly, we will need to
/// be able to have regular expressions after the dot star. And we will need to star regular
/// expressions other than dot.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Regex<Lbl> {
    labels: Vec<Lbl>,
    ends_in_dot_star: bool,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Extension<Lbl> {
    /// An empty regular expression
    Epsilon,
    /// A single label
    Label(Lbl),
    /// A dot star (matching 0 or more labels)
    DotStar,
}

//**************************************************************************************************
// impls
//**************************************************************************************************

impl<Lbl> Regex<Lbl> {
    /// An empty regular expression
    pub(crate) fn epsilon() -> Self {
        Self {
            labels: vec![],
            ends_in_dot_star: false,
        }
    }

    /// A single label
    pub(crate) fn label(lbl: Lbl) -> Self {
        Self {
            labels: vec![lbl],
            ends_in_dot_star: false,
        }
    }

    /// A dot star
    pub(crate) fn dot_star() -> Self {
        Self {
            labels: vec![],
            ends_in_dot_star: true,
        }
    }

    /// Returns true if the regex is empty (no labels and no dot star)
    pub(crate) fn is_epsilon(&self) -> bool {
        self.labels.is_empty() && !self.ends_in_dot_star
    }

    /// Used internally for metering of the graph within the bytecode verifier. We consider even
    /// an empty regex to be of size 1. Then we add one for each label and one for the dot star.
    /// In short, `abstract_size` is an abstraction over the Rust allocation size of the regex.
    pub(crate) fn abstract_size(&self) -> usize {
        1 + self.labels.len() + (self.ends_in_dot_star as usize)
    }

    /// Internal helper for the public facing API
    pub(crate) fn query_api_path(&self) -> (Vec<Lbl>, bool)
    where
        Lbl: Clone,
    {
        (self.labels.clone(), self.ends_in_dot_star)
    }

    /// This concatenates an extension onto the end of the regular expression.
    /// If the regular expression ends in dot star, then there is no current need to keep track of
    /// the labels after. The additional precision of having labels after the dot star is not
    /// needed currently in Move.
    pub(crate) fn extend(mut self, ext: &Extension<Lbl>) -> Self
    where
        Lbl: Clone,
    {
        match ext {
            _ if self.ends_in_dot_star => self,
            Extension::Epsilon => self,
            Extension::Label(lbl) => {
                self.labels.push(lbl.clone());
                self
            }
            Extension::DotStar => {
                self.ends_in_dot_star = true;
                self
            }
        }
    }

    /// If self = pq, then remove_prefix(p) returns Some(q) for all possible q.
    ///
    /// The input `p` is restricted to being an `Extension`. This is a practical limitation that
    /// makes writing the algorithm easier.
    ///
    /// Examples where there is a prefix, p:
    /// Remove epsilon
    /// "".remove_prefix("") = [""]
    /// "a".remove_prefix("") = ["a"]
    /// "ab".remove_prefix("") = ["ab"]
    /// ".*".remove_prefix("") = [".*"]
    /// "a.*".remove_prefix("") = ["a.*"]
    ///
    /// Remove label
    /// "ab".remove_prefix("a") = ["b"]
    /// "abc".remove_prefix("a") = ["bc"]
    /// ".*".remove_prefix("a") = [".*"]
    /// "a.*".remove_prefix("a") = [".*"]
    /// "ab.*".remove_prefix("a") = ["b.*"]
    ///
    /// Remove dot star
    /// "".remove_prefix(".*") = [""]
    /// "a".remove_prefix(".*") = ["a", ""]
    /// "ab".remove_prefix(".*") = ["ab", "b", ""]
    /// "abc".remove_prefix(".*") = ["abc", "bc", "c", ""]
    /// ".*".remove_prefix(".*") = [".*"]
    /// "a.*".remove_prefix(".*") = ["a.*", ".*", ""] ==> optimized to [".*"]
    /// "ab.*".remove_prefix(".*") = ["ab.*", "b.*", ".*", ""] ==> optimized to [".*"]
    ///
    /// Cases where there is no prefix:
    /// "".remove_prefix("a") = []
    /// "a".remove_prefix("b") = []
    /// "ab".remove_prefix("b") = []
    /// "abc".remove_prefix("b") = []
    /// "a.*".remove_prefix("b") = []
    pub(crate) fn remove_prefix(&self, p: &Extension<Lbl>) -> Vec<Regex<Lbl>>
    where
        Lbl: Clone,
        Lbl: Eq,
    {
        let mut self_walk = self.walk();
        match p {
            Extension::Epsilon => {
                let result = self_walk.remaining().into_iter().collect::<Vec<_>>();
                debug_assert!(!result.is_empty());
                result
            }
            Extension::Label(l_p) => {
                match self_walk.peek() {
                    WalkPeek::EmptySet => {
                        debug_assert!(false);
                        vec![]
                    }
                    WalkPeek::Epsilon => {
                        // cannot remove l1 from epsilon
                        vec![]
                    }
                    WalkPeek::Label(l_self) => {
                        if l_p != l_self {
                            // cannot remove l_p if it doesn't match l_self
                            vec![]
                        } else {
                            // we remove l_p and return the remaining
                            self_walk.next();
                            let result = self_walk.remaining().into_iter().collect::<Vec<_>>();
                            debug_assert!(!result.is_empty());
                            result
                        }
                    }
                    // we can remove any prefix and still have dot star
                    WalkPeek::DotStar => vec![Self::dot_star()],
                }
            }
            Extension::DotStar => {
                if self.ends_in_dot_star {
                    // This is an optimization for the case where we have a list of labels
                    // with a dot star. If this ends in dot star we will eventually add it to
                    // the result set, and `r | .*` is equivalent to `.*` so we don't need the
                    // partial paths
                    return vec![Self::dot_star()];
                }
                let mut result = vec![];
                while let Some(rem) = self_walk.remaining() {
                    result.push(rem);
                    self_walk.next();
                }
                debug_assert!(matches!(self_walk.peek(), WalkPeek::EmptySet));
                result
            }
        }
    }

    fn walk(&self) -> Walk<'_, Lbl> {
        if self.is_epsilon() {
            Walk::Epsilon
        } else {
            Walk::Regex {
                regex: self,
                idx: 0,
            }
        }
    }
}

impl<Lbl> Extension<Lbl> {
    /// If self = pq, then remove_prefix(p) returns Some(q) for all possible q.
    ///
    /// This is similar to `Regex::remove_prefix`, but with the restrictions flipped.
    ///
    /// Cases where there is a prefix, p:
    /// Removing from epsilon
    /// "".remove_prefix("") = [""]
    /// "".remove_prefix(".*") = [""]
    ///
    /// Removing from label
    /// "a".remove_prefix("") = ["a"]
    /// "a".remove_prefix("a") = [""]
    /// "a".remove_prefix(".*") = ["a", ""]
    /// "a".remove_prefix("a.*") = [""]
    ///
    /// Removing from dot star
    /// ".*".remove_prefix("") = [".*"]
    /// ".*".remove_prefix("a") = [".*"]
    /// ".*".remove_prefix("ab") = [".*"]
    /// ".*".remove_prefix(".*") = [".*"]
    /// ".*".remove_prefix("a.*") = [".*"]
    ///
    /// Cases where there is no prefix:
    /// "".remove_prefix("a") = []
    /// "".remove_prefix("ab") = []
    /// "".remove_prefix("a.*") = []
    /// "a".remove_prefix("b") = []
    /// "a".remove_prefix("bc.*") = []
    /// "a".remove_prefix("ab") = []
    pub(crate) fn remove_prefix(&self, p: &Regex<Lbl>) -> Vec<Regex<Lbl>>
    where
        Lbl: Clone,
        Lbl: Eq,
    {
        let mut p_walk = p.walk();
        match p_walk.peek() {
            WalkPeek::EmptySet => {
                debug_assert!(false);
                vec![]
            }
            WalkPeek::Epsilon => {
                // p = epsilon so q = self
                vec![self.clone().into_regex()]
            }
            WalkPeek::Label(l_p) => match self {
                Extension::Epsilon => {
                    // cannot remove l1 from epsilon
                    vec![]
                }
                Extension::Label(l_self) => {
                    if l_p != l_self {
                        // cannot remove l_p if it doesn't match l_self
                        vec![]
                    } else {
                        p_walk.next();
                        match p_walk.peek() {
                            WalkPeek::EmptySet => {
                                debug_assert!(false);
                                vec![]
                            }
                            WalkPeek::Epsilon | WalkPeek::DotStar => {
                                // p = l_p and q = epsilon (or the case where dot star is epsilon)
                                vec![Regex::epsilon()]
                            }
                            WalkPeek::Label(_) => {
                                // there is another label that we cannot remove from self
                                vec![]
                            }
                        }
                    }
                }
                // we can remove any prefix and still have dot star
                Extension::DotStar => vec![Regex::dot_star()],
            },
            WalkPeek::DotStar => {
                match self {
                    Extension::DotStar => vec![Regex::dot_star()],
                    Extension::Epsilon => {
                        // Consider the case where p=.*=epsilon so p and q are epsilon
                        vec![Regex::epsilon()]
                    }
                    Extension::Label(_) => {
                        // Two possibilities
                        // p = epsilon and q = self
                        // p = self and q = epsilon
                        vec![self.clone().into_regex(), Regex::epsilon()]
                    }
                }
            }
        }
    }

    pub(crate) fn into_regex(self) -> Regex<Lbl> {
        match self {
            Extension::Epsilon => Regex::epsilon(),
            Extension::Label(lbl) => Regex::label(lbl),
            Extension::DotStar => Regex::dot_star(),
        }
    }
}

// Helper for remove_prefix
enum Walk<'a, Lbl> {
    EmptySet,
    Epsilon,
    Regex { regex: &'a Regex<Lbl>, idx: usize },
}

// Helper for remove_prefix
enum WalkPeek<'a, Lbl> {
    EmptySet,
    Epsilon,
    Label(&'a Lbl),
    DotStar,
}

impl<Lbl> Walk<'_, Lbl> {
    fn peek(&self) -> WalkPeek<'_, Lbl> {
        match self {
            Walk::EmptySet => WalkPeek::EmptySet,
            Walk::Epsilon => WalkPeek::Epsilon,
            Walk::Regex { regex, idx } => {
                if *idx < regex.labels.len() {
                    WalkPeek::Label(&regex.labels[*idx])
                } else if regex.ends_in_dot_star {
                    WalkPeek::DotStar
                } else {
                    debug_assert!(false);
                    WalkPeek::Epsilon
                }
            }
        }
    }

    fn next(&mut self) {
        match self {
            Walk::EmptySet => {
                debug_assert!(false);
            }
            Walk::Epsilon => {
                *self = Walk::EmptySet;
            }
            Walk::Regex { regex, idx } => {
                if *idx < regex.labels.len() {
                    *idx += 1;
                }
                if *idx >= regex.labels.len() && !regex.ends_in_dot_star {
                    *self = Walk::Epsilon;
                }
            }
        }
    }

    fn remaining(&self) -> Option<Regex<Lbl>>
    where
        Lbl: Clone,
    {
        match self {
            Walk::EmptySet => None,
            Walk::Epsilon => Some(Regex::epsilon()),
            Walk::Regex { regex, idx } => {
                let idx = *idx;
                let labels = if idx < regex.labels.len() {
                    regex.labels[idx..].to_vec()
                } else {
                    debug_assert!(regex.ends_in_dot_star);
                    vec![]
                };
                let ends_in_dot_star = regex.ends_in_dot_star;
                Some(Regex {
                    labels,
                    ends_in_dot_star,
                })
            }
        }
    }
}

//**************************************************************************************************
// traits
//**************************************************************************************************

/// Helper macro to help with Debug and Display. Cannot be a function since we cannot write
/// `Lbl: Display OR Debug`
macro_rules! fmt_regex {
    ($f:expr, $path:expr) => {{
        let f = $f;
        let p = $path;
        if p.is_epsilon() {
            return write!(f, "ε");
        }
        let mut exts = p.labels.iter().peekable();
        while let Some(ext) = exts.peek() {
            // display the element
            ext.fmt(f)?;
            // advance the iterator
            exts.next();
            // if there is a next element, add a separator
            if exts.peek().is_some() {
                write!(f, ".")?;
            }
        }
        if p.ends_in_dot_star {
            // we use _ instead of . to avoid confusion with field access/extension
            write!(f, "_*")?;
        }
        Ok(())
    }};
}

impl<Lbl> Display for Regex<Lbl>
where
    Lbl: Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt_regex!(f, self)
    }
}

impl<Lbl> Debug for Regex<Lbl>
where
    Lbl: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt_regex!(f, self)
    }
}
