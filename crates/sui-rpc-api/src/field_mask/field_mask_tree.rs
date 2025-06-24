// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::is_valid_path;
use super::FieldMaskUtil;
use super::FIELD_PATH_SEPARATOR;
use super::FIELD_PATH_WILDCARD;
use super::FIELD_SEPARATOR;

use prost_types::FieldMask;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct FieldMaskTree {
    wildcard: bool,
    root: Node,
}

#[derive(Clone, Debug, Default)]
struct Node {
    children: BTreeMap<String, Node>,
}

impl FieldMaskTree {
    pub fn add_field_path(&mut self, path: &str) -> &mut Self {
        if self.wildcard || !is_valid_path(path) {
            return self;
        }

        if path == FIELD_PATH_WILDCARD {
            self.wildcard = true;
            self.root.children.clear();
            return self;
        }

        let root = std::ptr::from_ref(&self.root);
        let mut node = &mut self.root;
        let mut create_new_branch = false;
        for component in path.split(FIELD_SEPARATOR) {
            if !create_new_branch && !std::ptr::eq(root, node) && node.children.is_empty() {
                return self;
            }

            node = node
                .children
                .entry(component.to_owned())
                .or_insert_with(|| {
                    create_new_branch = true;
                    Node::default()
                });
        }

        node.children.clear();
        self
    }

    pub fn from_field_mask(mask: &FieldMask) -> Self {
        let mut tree = Self::default();
        for path in &mask.paths {
            tree.add_field_path(path);
        }
        tree
    }

    pub fn to_field_mask(&self) -> FieldMask {
        if self.root.children.is_empty() {
            return FieldMask::default();
        }

        let mut paths = Vec::new();
        Self::collect_field_paths(&self.root, &mut String::new(), &mut paths);
        FieldMask { paths }
    }

    fn collect_field_paths(node: &Node, path: &mut String, paths: &mut Vec<String>) {
        if node.children.is_empty() {
            paths.push(path.clone());
            return;
        }

        let parent_path_len = path.len();
        for (part, child) in node.children.iter() {
            if path.is_empty() {
                path.push_str(part);
            } else {
                path.push(FIELD_SEPARATOR);
                path.push_str(part);
            };
            Self::collect_field_paths(child, path, paths);
            path.truncate(parent_path_len);
        }
    }

    /// Checks if the provided path is contained in this FieldMaskTree.
    ///
    /// A path is considered a match and contained by this tree if it is a prefix for any contained
    /// paths, including if it is an exact match.
    ///
    /// ```
    /// # use sui_rpc_api::field_mask::FieldMaskTree;
    /// let mut tree = FieldMaskTree::default();
    /// tree.add_field_path("foo.bar");
    ///
    /// assert!(tree.contains("foo"));
    /// assert!(tree.contains("foo.bar"));
    /// assert!(!tree.contains("foo.baz"));
    /// ```
    pub fn contains(&self, path: &str) -> bool {
        if path.is_empty() {
            return false;
        }

        if self.wildcard {
            return true;
        }

        let mut node = &self.root;
        for component in path.split(FIELD_SEPARATOR) {
            // If this isn't the root node, and there are no sub-paths, then this path has been
            // matched and we can return a hit
            if !std::ptr::eq(node, &self.root) && node.children.is_empty() {
                return true;
            }

            if let Some(child) = node.children.get(component) {
                node = child;
            } else {
                return false;
            }
        }

        // We found a matching node for this path. This node may be empty or have leaf children. In
        // either case the provided patch is a "match" and is contained by this tree.
        true
    }

    pub fn subtree(&self, path: &str) -> Option<Self> {
        if path.is_empty() {
            return None;
        }

        if self.wildcard {
            return Some(self.clone());
        }

        let mut node = &self.root;
        for component in path.split(FIELD_SEPARATOR) {
            if let Some(child) = node.children.get(component) {
                node = child;
            } else {
                return None;
            }
        }

        if std::ptr::eq(node, &self.root) {
            None
        } else {
            Some(Self {
                wildcard: node.children.is_empty(),
                root: node.clone(),
            })
        }
    }

    pub(crate) fn new_wildcard() -> Self {
        Self {
            wildcard: true,
            root: Default::default(),
        }
    }
}

impl From<FieldMask> for FieldMaskTree {
    fn from(mask: FieldMask) -> Self {
        Self::from_field_mask(&mask)
    }
}

impl From<FieldMaskTree> for FieldMask {
    fn from(tree: FieldMaskTree) -> Self {
        tree.to_field_mask()
    }
}

impl std::fmt::Display for FieldMaskTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        FieldMaskUtil::display(&self.to_field_mask()).fmt(f)
    }
}

impl std::str::FromStr for FieldMaskTree {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut tree = Self::default();

        for path in s.split(FIELD_PATH_SEPARATOR) {
            tree.add_field_path(path);
        }

        Ok(tree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_field_path() {
        let mut tree = FieldMaskTree::default();

        assert!(tree.to_string().is_empty());
        tree.add_field_path("");
        assert!(tree.to_string().is_empty());

        tree.add_field_path("foo");
        assert_eq!(tree.to_string(), "foo");
        // redundant path
        tree.add_field_path("foo");
        assert_eq!(tree.to_string(), "foo");

        tree.add_field_path("bar.baz");
        assert_eq!(tree.to_string(), "bar.baz,foo");

        // redundant sub-path
        tree.add_field_path("foo.bar");
        assert_eq!(tree.to_string(), "bar.baz,foo");

        // new sub-path
        tree.add_field_path("bar.quz");
        assert_eq!(tree.to_string(), "bar.baz,bar.quz,foo");

        // path that matches several existing sub-paths
        tree.add_field_path("bar");
        assert_eq!(tree.to_string(), "bar,foo");
    }

    #[test]
    fn test_contains() {
        let mut tree = FieldMaskTree::default();
        tree.add_field_path("foo.bar");

        assert!(tree.contains("foo"));
        assert!(tree.contains("foo.bar"));
        assert!(!tree.contains("foo.baz"));
        assert!(!tree.contains("foobar"));
    }
}
