// Copyright (c) 2020 Christian Vallentin
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, VecDeque};

/// [Breadth-First Traversal] (or Level Order Traversal).
///
/// [Breadth-First Traversal]: https://en.wikipedia.org/wiki/Tree_traversal
///
/// # Cycles
///
/// `Bft` does not handle cycles. If any
/// cycles are present, then `Bft` will
/// result in an infinite (never ending)
/// [`Iterator`].
///
/// [`Iterator`]: https://doc.rust-lang.org/stable/std/iter/trait.Iterator.html
///
/// # Example
///
/// ```
/// use dag::bft::Bft;
///
/// #[derive(PartialEq, Eq, Hash)]
/// struct Node(&'static str, &'static [Node]);
///
/// let tree = Node("A", &[
///     Node("B", &[
///         Node("D", &[]),
///         Node("E", &[])
///     ]),
///     Node("C", &[
///         Node("F", &[]),
///         Node("G", &[])
///     ]),
/// ]);
///
/// // `&tree` represents the root `Node`.
/// // The `Fn(&Node) -> Iterator<Item = &Node>` returns
/// // an `Iterator` to get the child `Node`s.
/// let iter = Bft::new(&tree, |node| node.1.iter());
///
/// // Map `Iterator<Item = &Node>` into `Iterator<Item = &str>`
/// let mut iter = iter.map(|node| node.0);
///
/// assert_eq!(iter.next(), Some(("A")));
/// assert_eq!(iter.next(), Some(("B")));
/// assert_eq!(iter.next(), Some(("C")));
/// assert_eq!(iter.next(), Some(("D")));
/// assert_eq!(iter.next(), Some(("E")));
/// assert_eq!(iter.next(), Some(("F")));
/// assert_eq!(iter.next(), Some(("G")));
/// assert_eq!(iter.next(), None);
/// ```
#[derive(Clone, Debug)]
pub struct Bft<T, F, I>
where
    F: Fn(&T) -> I,
    I: Iterator<Item = T>,
{
    queue: VecDeque<T>,
    visited: BTreeSet<u64>,
    iter_children: F,
}

fn default_hash<T: std::hash::Hash>(val: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    val.hash(&mut hasher);
    std::hash::Hasher::finish(&hasher)
}

impl<T, F, I> Bft<T, F, I>
where
    F: Fn(&T) -> I,
    I: Iterator<Item = T>,
{
    /// Creates a `Bft`, where `root` is the
    /// starting `Node`.
    ///
    /// The `iter_children` [`Fn`] is (lazily) called
    /// for each `Node` as needed, where the
    /// returned [`Iterator`] produces the child
    /// `Node`s for the given `Node`.
    ///
    /// [`Iterator`]: https://doc.rust-lang.org/stable/std/iter/trait.Iterator.html
    ///
    /// *[See `Bft` for more information.][`Bft`]*
    ///
    /// [`Bft`]: struct.Bft.html
    ///
    #[inline]
    pub fn new(root: T, iter_children: F) -> Self {
        Self {
            queue: VecDeque::from(vec![root]),
            visited: BTreeSet::new(),
            iter_children,
        }
    }
}

impl<T, F, I> Iterator for Bft<T, F, I>
where
    T: Eq + std::hash::Hash,
    F: Fn(&T) -> I,
    I: Iterator<Item = T>,
{
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(node) = self.queue.pop_front() {
            let children = (self.iter_children)(&node);
            for c in children {
                let h = default_hash(&c);
                if !self.visited.contains(&h) {
                    self.visited.insert(h);
                    self.queue.push_back(c)
                }
            }

            Some(node)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Eq, PartialEq, Hash)]
    struct Node(&'static str, &'static [Node]);

    #[test]
    fn bft() {
        #[rustfmt::skip]
        let tree = Node("A", &[
            Node("B", &[
                Node("D", &[]),
                Node("E", &[
                    Node("H", &[])
                ])]),
            Node("C", &[
                Node("F", &[
                    Node("I", &[])
                ]),
                Node("G", &[])]),
        ]);

        let iter = Bft::new(&tree, |node| node.1.iter());
        let mut iter = iter.map(|node| node.0);

        assert_eq!(iter.next(), Some("A"));
        assert_eq!(iter.next(), Some("B"));
        assert_eq!(iter.next(), Some("C"));
        assert_eq!(iter.next(), Some("D"));
        assert_eq!(iter.next(), Some("E"));
        assert_eq!(iter.next(), Some("F"));
        assert_eq!(iter.next(), Some("G"));
        assert_eq!(iter.next(), Some("H"));
        assert_eq!(iter.next(), Some("I"));
        assert_eq!(iter.next(), None);
    }
}
