// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, rc::Rc};

use anyhow::anyhow;
use async_graphql::{
    parser::types::{
        ExecutableDocument, Field, FragmentDefinition, OperationDefinition, OperationType,
        Selection, SelectionSet,
    },
    registry::{MetaField, MetaType, Registry},
    Name, Pos, Positioned, Variables,
};
use async_graphql_value::{ConstValue, Value};

use super::{
    chain::Chain,
    error::{Error, ErrorKind},
};

/// Visitors traverse the AST of a query, performing an action on each field.
pub(super) trait Visitor<'r> {
    /// This function is called on each field in the query.
    ///
    /// The `driver` parameter gives access to the field's value and type information, and controls
    /// when and how to recurse into nested field selections.
    fn visit_field<'t>(&mut self, driver: &FieldDriver<'t, 'r>) -> Result<(), Error>;
}

/// Like [Driver] but specialized to a specific field.
///
/// ## Lifetimes
/// - `'t`: The lifetime of the traversal.
/// - `'r`: The lifetime of the request.
pub(super) struct FieldDriver<'t, 'r> {
    driver: &'t Driver<'r>,
    chain: Option<Rc<Chain>>,
    meta_type: &'r MetaType,
    meta_field: &'r MetaField,
    field: &'r Positioned<Field>,
}

/// The driver abstracts over recursively visiting a GraphQL document. It keeps track of type
/// information, path and source position for the current field.
pub(super) struct Driver<'r> {
    registry: &'r Registry,
    fragments: &'r HashMap<Name, Positioned<FragmentDefinition>>,
    variables: &'r Variables,
}

impl<'r> FieldDriver<'_, 'r> {
    /// Metadata about the field's parent type.
    pub(super) fn parent_type(&self) -> &'r MetaType {
        self.meta_type
    }

    /// Metadata about the field.
    pub(super) fn meta_field(&self) -> &'r MetaField {
        self.meta_field
    }

    /// The field's AST node.
    pub(super) fn field(&self) -> &'r Positioned<Field> {
        self.field
    }

    /// Find an argument on the current field by its name, and return its fully resolved value if
    /// it exists, or `None` if it does not. Fails if the argument references a GraphQL variable
    /// that has not been bound.
    pub(super) fn resolve_arg(&self, name: &str) -> Result<Option<ConstValue>, Error> {
        let Some(val) = self
            .field
            .node
            .arguments
            .iter()
            .find_map(|(n, v)| (n.node.as_str() == name).then_some(&v.node))
        else {
            return Ok(None);
        };

        Ok(Some(self.resolve_val(val.clone())?))
    }

    /// Return `val` with variables all resolved. Fails if a referenced variable is not found.
    pub(super) fn resolve_val(&self, val: Value) -> Result<ConstValue, Error> {
        val.into_const_with(|name| {
            self.resolve_var(&name)
                .ok_or_else(|| self.err(ErrorKind::VariableNotFound(name)))
                .cloned()
        })
    }

    /// Find the value bound to a given GraphQL variable name.
    pub(super) fn resolve_var(&self, name: &Name) -> Option<&'r ConstValue> {
        self.driver.variables.get(name)
    }

    /// Helper to contextualize an error with the field's position and path.
    pub(super) fn err(&self, kind: ErrorKind) -> Error {
        self.err_at(self.field.pos, kind)
    }

    /// Helper to contextualize an error with the field's path and a custom position.
    pub(super) fn err_at(&self, pos: Pos, kind: ErrorKind) -> Error {
        Error::new(kind, Chain::path(&self.chain), pos)
    }

    /// Continue traversing the query, visiting the selection of fields nested within these field,
    /// if there are any.
    pub(super) fn visit_selection_set<V: Visitor<'r> + ?Sized>(
        &self,
        visitor: &mut V,
    ) -> Result<(), Error> {
        self.driver.visit_selection_set(
            &self.meta_field.ty,
            self.chain.clone(),
            &self.field.node.selection_set,
            visitor,
        )
    }
}

impl<'r> Driver<'r> {
    /// Entry point for visiting a document with a particular visitor.
    pub(super) fn visit_document<V: Visitor<'r> + ?Sized>(
        registry: &'r Registry,
        doc: &'r ExecutableDocument,
        variables: &'r Variables,
        visitor: &mut V,
    ) -> Result<(), Error> {
        let driver = Self {
            registry,
            fragments: &doc.fragments,
            variables,
        };

        for (name, op) in doc.operations.iter() {
            driver.visit_operation(name, op, visitor)?;
        }

        Ok(())
    }

    fn visit_operation<V: Visitor<'r> + ?Sized>(
        &self,
        _name: Option<&Name>,
        op: &'r Positioned<OperationDefinition>,
        visitor: &mut V,
    ) -> Result<(), Error> {
        let Some(root) = (match op.node.ty {
            OperationType::Query => Some(&self.registry.query_type),
            OperationType::Mutation => self.registry.mutation_type.as_ref(),
            OperationType::Subscription => self.registry.subscription_type.as_ref(),
        }) else {
            return Err(Error::new_global(ErrorKind::SchemaNotSupported(op.node.ty)));
        };

        self.visit_selection_set(root, None, &op.node.selection_set, visitor)
    }

    fn visit_selection_set<V: Visitor<'r> + ?Sized>(
        &self,
        type_: &'r str,
        chain: Option<Rc<Chain>>,
        selection_set: &'r Positioned<SelectionSet>,
        visitor: &mut V,
    ) -> Result<(), Error> {
        let Some(meta_type) = self.registry.concrete_type_by_name(type_) else {
            return Err(Error::new(
                ErrorKind::InternalError(anyhow!("Type '{type_}' not found in schema")),
                Chain::path(&chain),
                selection_set.pos,
            ));
        };

        for sel in &selection_set.node.items {
            match &sel.node {
                Selection::Field(field) => {
                    let name = &field.node.name.node;
                    let meta_field = meta_type.field_by_name(name.as_str()).ok_or_else(|| {
                        Error::new(
                            ErrorKind::InternalError(anyhow!(
                                "Field '{type_}.{name}' not found in schema"
                            )),
                            Chain::path(&chain),
                            field.pos,
                        )
                    })?;

                    visitor.visit_field(&FieldDriver {
                        driver: self,
                        chain: Some(Chain::new(chain.clone(), name.clone())),
                        meta_type,
                        meta_field,
                        field,
                    })?;
                }

                Selection::FragmentSpread(fragment_spread) => {
                    let name = &fragment_spread.node.fragment_name.node;
                    let fragment_def = self.fragments.get(name).ok_or_else(|| {
                        Error::new(
                            ErrorKind::UnknownFragment(name.as_str().to_owned()),
                            Chain::path(&chain),
                            fragment_spread.pos,
                        )
                    })?;

                    self.visit_selection_set(
                        fragment_def.node.type_condition.node.on.node.as_str(),
                        chain.clone(),
                        &fragment_def.node.selection_set,
                        visitor,
                    )?;
                }

                Selection::InlineFragment(inline_fragment) => {
                    self.visit_selection_set(
                        inline_fragment
                            .node
                            .type_condition
                            .as_ref()
                            .map_or(type_, |cond| cond.node.on.node.as_str()),
                        chain.clone(),
                        &inline_fragment.node.selection_set,
                        visitor,
                    )?;
                }
            }
        }

        Ok(())
    }
}
