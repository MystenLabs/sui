// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module defines a test helper for the abstract state that can be easily serialized into a
//! string. The main purpose of this is to be able to see the abstract states at the beginning of
//! each block in the test output.

use std::collections::{BTreeMap, BTreeSet};

use move_binary_format::file_format::{
    EnumDefinitionIndex, FieldHandleIndex, LocalIndex, MemberCount, VariantTag,
};
use move_regex_borrow_graph::references::Ref;
use serde::{Deserialize, Serialize};

use crate::regex_reference_safety::abstract_state::{AbstractState, Graph, Label};

/// A trait used to populate the `AbstractState` with human readable names and labels.
pub trait StateSerializer {
    fn local_root(&mut self, r: Ref) -> String;
    fn ref_(&mut self, idx: LocalIndex, r: Ref) -> String;
    fn local(&mut self, idx: LocalIndex) -> String;
    fn label_local(&mut self, idx: LocalIndex) -> String;
    fn label_field(&mut self, idx: FieldHandleIndex) -> String;
    fn label_variant_field(
        &mut self,
        enum_def_idx: EnumDefinitionIndex,
        tag: VariantTag,
        field_idx: MemberCount,
    ) -> String;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SerializableState {
    pub local_root: String,
    pub locals: BTreeMap<String, SerializableValue>,
    pub graph: SerializableGraph,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum SerializableValue {
    Reference(String),
    NonReference,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SerializableGraph {
    pub nodes: BTreeMap<String, /* is_mut */ bool>,
    /// Skips self epsilon edges
    pub outgoing: BTreeMap<String, BTreeMap<String, BTreeSet<SerializableEdge>>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct SerializableEdge {
    pub labels: Vec<String>,
    pub ends_in_dot_star: bool,
}

impl AbstractState {
    pub fn to_serializable(&self, serializer: &mut impl StateSerializer) -> SerializableState {
        let mut refs = BTreeMap::new();
        let local_root = serializer.local_root(self.local_root());
        refs.insert(self.local_root(), local_root.clone());
        let locals = locals_to_serializable(self.locals(), &mut refs, serializer);
        let graph = graph_to_serializable(self.graph(), &refs, serializer);
        SerializableState {
            local_root,
            locals,
            graph,
        }
    }
}

fn locals_to_serializable(
    locals: &BTreeMap<LocalIndex, Ref>,
    acc: &mut BTreeMap<Ref, String>,
    serializer: &mut impl StateSerializer,
) -> BTreeMap<String, SerializableValue> {
    locals
        .iter()
        .map(|(idx, r)| {
            let key = serializer.local(*idx);
            let value = serializer.ref_(*idx, *r);
            acc.insert(*r, value.clone());
            // TODO support non-reference locals
            (key, SerializableValue::Reference(value))
        })
        .collect()
}

fn graph_to_serializable(
    graph: &Graph,
    refs: &BTreeMap<Ref, String>,
    serializer: &mut impl StateSerializer,
) -> SerializableGraph {
    let nodes = graph
        .keys()
        .map(|r| {
            let key = refs.get(&r).unwrap().clone();
            let is_mut = graph.is_mutable(r).unwrap();
            (key, is_mut)
        })
        .collect();

    let outgoing = graph
        .keys()
        .filter_map(|source| {
            let borrowed_by = graph.borrowed_by(source).unwrap();
            if borrowed_by.is_empty() {
                return None;
            }
            let source_key = refs.get(&source).unwrap().clone();
            let targets: BTreeMap<String, BTreeSet<SerializableEdge>> = borrowed_by
                .into_iter()
                .map(|(target, paths)| {
                    let target_key = refs.get(&target).unwrap().clone();
                    let edges: BTreeSet<SerializableEdge> = paths
                        .into_iter()
                        .map(|path| path_to_serializable_edge(path, serializer))
                        .collect();
                    (target_key, edges)
                })
                .collect();
            Some((source_key, targets))
        })
        .collect();

    SerializableGraph { nodes, outgoing }
}

fn path_to_serializable_edge(
    path: move_regex_borrow_graph::collections::Path<(), Label>,
    serializer: &mut impl StateSerializer,
) -> SerializableEdge {
    let labels = path
        .labels
        .into_iter()
        .map(|lbl| label_to_string(lbl, serializer))
        .collect();
    SerializableEdge {
        labels,
        ends_in_dot_star: path.ends_in_dot_star,
    }
}

fn label_to_string(label: Label, serializer: &mut impl StateSerializer) -> String {
    match label {
        Label::Local(idx) => serializer.label_local(idx),
        Label::Field(idx) => serializer.label_field(idx),
        Label::VariantField(enum_def_idx, tag, field_idx) => {
            serializer.label_variant_field(enum_def_idx, tag, field_idx)
        }
    }
}
