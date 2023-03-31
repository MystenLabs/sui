// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod access;

pub use access::Access;

use move_binary_format::normalized::Type;
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ResourceKey, TypeTag},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::btree_map::BTreeMap,
    fmt::{self, Formatter},
};

/// Offset of an access path: either a field, vector index, or global key
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Offset {
    /// Index into contents of a struct by field offset
    Field(usize),
    /// Unknown index into a vector
    VectorIndex,
    /// A type index into global storage. Only follows a field or vector index of type address
    Global(Type),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrieNode {
    /// Optional data associated with the parent in the trie
    data: Option<Access>,
    /// Child pointers labeled by offsets
    children: BTreeMap<Offset, TrieNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RootAddress {
    Const(AccountAddress),
    Formal(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Root {
    pub root: RootAddress,
    pub type_: Type,
}

#[derive(Debug, Clone)]
pub struct AccessPath {
    pub root: Root,
    pub offsets: Vec<Offset>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadWriteSet(BTreeMap<Root, TrieNode>);

impl Root {
    fn subst_actuals(&self, type_actuals: &[Type], actuals: &[Option<AccountAddress>]) -> Self {
        let root_address = match &self.root {
            RootAddress::Const(addr) => RootAddress::Const(*addr),
            RootAddress::Formal(i) => {
                if let Some(addr) = actuals.get(*i).and_then(|v| *v) {
                    RootAddress::Const(addr)
                } else {
                    panic!("Type parameter index out of bound")
                }
            }
        };
        Self {
            root: root_address,
            type_: self.type_.subst(type_actuals),
        }
    }
}

impl AccessPath {
    pub fn offset(&self) -> &[Offset] {
        self.offsets.as_slice()
    }
    pub fn root(&self) -> &Root {
        &self.root
    }
    pub fn add_offset(&mut self, offset: Offset) {
        self.offsets.push(offset)
    }
    pub fn new_global_constant(addr: AccountAddress, ty: Type) -> Self {
        Self {
            root: Root {
                root: RootAddress::Const(addr),
                type_: ty,
            },
            offsets: vec![],
        }
    }
    pub fn has_secondary_index(&self) -> bool {
        self.offsets.iter().any(|offset| match offset {
            Offset::Global(_) => true,
            Offset::Field(_) | Offset::VectorIndex => false,
        })
    }
}

impl Offset {
    fn sub_type_actuals(&self, type_actuals: &[Type]) -> Self {
        match self {
            Offset::Global(s) => Offset::Global(s.subst(type_actuals)),
            Offset::Field(_) | Offset::VectorIndex => self.clone(),
        }
    }
}

impl TrieNode {
    pub fn new() -> Self {
        Self {
            data: None,
            children: BTreeMap::new(),
        }
    }

    fn iter_paths_opt<F>(&self, access_path: &AccessPath, f: &mut F)
    where
        F: FnMut(&AccessPath, &Access),
    {
        if let Some(access) = &self.data {
            f(access_path, access);
        }
        for (k, v) in self.children.iter() {
            let mut new_ap = access_path.clone();
            new_ap.offsets.push(k.clone());
            v.iter_paths_opt(&new_ap, f)
        }
    }

    fn sub_type_actuals(&self, type_actuals: &[Type]) -> Self {
        Self {
            data: self.data,
            children: self
                .children
                .iter()
                .map(|(offset, node)| {
                    (
                        offset.sub_type_actuals(type_actuals),
                        node.sub_type_actuals(type_actuals),
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn get_access(&self) -> Option<Access> {
        self.get_access_impl(None)
    }

    fn get_access_impl(&self, mut acc: Option<Access>) -> Option<Access> {
        acc = match (self.data, acc) {
            (Some(lhs), Some(rhs)) => {
                if lhs != rhs {
                    Some(Access::ReadWrite)
                } else {
                    Some(lhs)
                }
            }
            (Some(_), None) => self.data,
            (None, _) => acc,
        };
        for (_, children) in self.children.iter() {
            acc = children.get_access_impl(acc)
        }
        acc
    }
}

impl ReadWriteSet {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn trim(&self) -> Self {
        Self(
            self.0
                .iter()
                .map(|(root, node)| {
                    (
                        root.clone(),
                        TrieNode {
                            data: node.get_access(),
                            children: BTreeMap::new(),
                        },
                    )
                })
                .collect(),
        )
    }

    pub fn add_access_path(&mut self, access_path: AccessPath, access: Access) {
        let mut node = self.0.entry(access_path.root).or_insert_with(TrieNode::new);
        for offset in access_path.offsets {
            node = node.children.entry(offset).or_insert_with(TrieNode::new);
        }
        node.data = Some(access);
    }

    fn iter_paths_impl<F>(&self, mut f: F) -> Option<()>
    where
        F: FnMut(&AccessPath, &Access) -> Option<()>,
    {
        let mut result = Some(());
        for (key, node) in self.0.iter() {
            let access_path = AccessPath {
                root: key.clone(),
                offsets: vec![],
            };
            node.iter_paths_opt(&access_path, &mut |access_path, access| {
                if result.is_none() {
                    return;
                }
                result = f(access_path, access);
            });
        }
        result
    }

    pub fn iter_paths<F>(&self, f: F) -> Option<()>
    where
        F: FnMut(&AccessPath, &Access) -> Option<()>,
    {
        self.iter_paths_impl(f)
    }

    fn get_keys_(&self, is_write: bool) -> Option<Vec<ResourceKey>> {
        let mut results = vec![];
        for (key, node) in self.0.iter() {
            let keep = match node.get_access() {
                Some(access) => {
                    if is_write {
                        access.is_write()
                    } else {
                        access.is_read()
                    }
                }
                None => false,
            };
            if keep {
                match key.root {
                    RootAddress::Const(addr) => {
                        results.push(ResourceKey::new(addr, key.type_.clone().into_struct_tag()?))
                    }
                    RootAddress::Formal(_) => return None,
                }
            }
        }
        Some(results)
    }

    pub fn get_keys_written(&self) -> Option<Vec<ResourceKey>> {
        self.get_keys_(true)
    }

    pub fn get_keys_read(&self) -> Option<Vec<ResourceKey>> {
        self.get_keys_(false)
    }

    pub fn sub_actuals(
        &self,
        actuals: &[Option<AccountAddress>],
        type_actuals: &[TypeTag],
    ) -> Self {
        let types: Vec<_> = type_actuals
            .iter()
            .map(|ty| Type::from(ty.clone()))
            .collect();
        Self(
            self.0
                .iter()
                .map(|(root, node)| {
                    (
                        root.subst_actuals(&types, actuals),
                        node.sub_type_actuals(&types),
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        )
    }
}

impl fmt::Display for Root {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.root {
            RootAddress::Const(addr) => write!(f, "0x{}", addr.short_str_lossless())?,
            RootAddress::Formal(i) => write!(f, "Formal({})", i)?,
        };
        write!(f, "/{}", self.type_)
    }
}
impl fmt::Display for Offset {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Offset::Global(ty) => write!(f, "{}", ty),
            Offset::VectorIndex => write!(f, "[_]"),
            Offset::Field(i) => write!(f, "{:?}", i),
        }
    }
}
impl fmt::Display for AccessPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.root)?;
        for offset in &self.offsets {
            f.write_str("/")?;
            write!(f, "{}", offset)?;
        }
        Ok(())
    }
}

impl fmt::Display for ReadWriteSet {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.iter_paths(|path, v| writeln!(f, "{}: {:?}", path, v).ok());
        Ok(())
    }
}
