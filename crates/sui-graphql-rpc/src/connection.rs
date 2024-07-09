// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::marker::PhantomData;

use async_graphql::connection::{
    ConnectionNameType, CursorType, DefaultConnectionName, DefaultEdgeName, Edge, EdgeNameType,
    EmptyFields, EnableNodesField, NodesFieldSwitcherSealed, PageInfo,
};
use async_graphql::{Object, ObjectType, OutputType, TypeName};

pub(crate) struct Connection<
    Cursor,
    Node,
    EdgeFields = EmptyFields,
    Name = DefaultConnectionName,
    EdgeName = DefaultEdgeName,
    NodesField = EnableNodesField,
> where
    Cursor: CursorType + Send + Sync,
    Node: OutputType,
    EdgeFields: ObjectType,
    Name: ConnectionNameType,
    EdgeName: EdgeNameType,
    NodesField: NodesFieldSwitcherSealed,
{
    _mark1: PhantomData<Name>,
    _mark2: PhantomData<EdgeName>,
    _mark3: PhantomData<NodesField>,
    pub edges: Vec<Edge<Cursor, Node, EdgeFields, EdgeName>>,
    pub has_previous_page: bool,
    pub has_next_page: bool,
    pub start_cursor: Option<String>,
    pub end_cursor: Option<String>,
}

#[Object(name_type)]
impl<Cursor, Node, EdgeFields, Name, EdgeName>
    Connection<Cursor, Node, EdgeFields, Name, EdgeName, EnableNodesField>
where
    Cursor: CursorType + Send + Sync,
    Node: OutputType,
    EdgeFields: ObjectType,
    Name: ConnectionNameType,
    EdgeName: EdgeNameType,
{
    /// Information to aid in pagination.
    async fn page_info(&self) -> PageInfo {
        // Unlike the default implementation, this Connection will use `start_cursor` and
        // `end_cursor` if they are `Some`.
        PageInfo {
            has_previous_page: self.has_previous_page,
            has_next_page: self.has_next_page,
            start_cursor: self
                .start_cursor
                .clone()
                .or_else(|| self.edges.first().map(|edge| edge.cursor.encode_cursor())),
            end_cursor: self
                .end_cursor
                .clone()
                .or_else(|| self.edges.last().map(|edge| edge.cursor.encode_cursor())),
        }
    }

    /// A list of edges.
    #[inline]
    async fn edges(&self) -> &[Edge<Cursor, Node, EdgeFields, EdgeName>] {
        &self.edges
    }

    /// A list of nodes.
    async fn nodes(&self) -> Vec<&Node> {
        self.edges.iter().map(|e| &e.node).collect()
    }
}

impl<Cursor, Node, NodesField, EdgeFields, Name, EdgeName>
    Connection<Cursor, Node, EdgeFields, Name, EdgeName, NodesField>
where
    Cursor: CursorType + Send + Sync,
    Node: OutputType,
    EdgeFields: ObjectType,
    Name: ConnectionNameType,
    EdgeName: EdgeNameType,
    NodesField: NodesFieldSwitcherSealed,
{
    /// Create a new connection.
    #[inline]
    pub fn new(has_previous_page: bool, has_next_page: bool) -> Self {
        Connection {
            _mark1: PhantomData,
            _mark2: PhantomData,
            _mark3: PhantomData,
            edges: Vec::new(),
            has_previous_page,
            has_next_page,
            start_cursor: None,
            end_cursor: None,
        }
    }
}

impl<Cursor, Node, EdgeFields, Name, EdgeName, NodesField> TypeName
    for Connection<Cursor, Node, EdgeFields, Name, EdgeName, NodesField>
where
    Cursor: CursorType + Send + Sync,
    Node: OutputType,
    EdgeFields: ObjectType,
    Name: ConnectionNameType,
    EdgeName: EdgeNameType,
    NodesField: NodesFieldSwitcherSealed,
{
    #[inline]
    fn type_name() -> Cow<'static, str> {
        Name::type_name::<Node>().into()
    }
}
