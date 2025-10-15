// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Cow,
    fmt::Write as _,
    sync::{Arc, atomic::AtomicUsize},
};

use futures::{
    future::{OptionFuture, join_all},
    join,
};
use move_core_types::account_address::AccountAddress;
use sui_types::dynamic_field::{
    DynamicFieldInfo, DynamicFieldType, derive_dynamic_field_id,
    visitor::{self as DFV, FieldVisitor},
};

use super::{
    error::FormatError,
    parser as P,
    value::{Accessor, Enum, Fields, Slice, Store, Struct, Value, Vector},
    visitor::extractor::Extractor,
    writer::BoundedWriter,
};

/// The interpreter is responsible for evaluating expressions inside format strings into values.
pub(crate) struct Interpreter<'s, S: Store<'s>> {
    root: Slice<'s>,
    store: S,
    max_output_size: usize,
    used_output: AtomicUsize,
}

impl<'s, S: Store<'s>> Interpreter<'s, S> {
    pub(crate) fn new(root: Slice<'s>, store: S, max_output_size: usize) -> Self {
        Self {
            root,
            store,
            max_output_size,
            used_output: AtomicUsize::new(0),
        }
    }

    /// Entrypoint to evaluate a single format string, represented as a sequence of its strands.
    pub(crate) async fn eval(
        &self,
        strands: &'s [P::Strand<'s>],
    ) -> Result<serde_json::Value, FormatError> {
        // TODO(amnn): Support nested display and JSON transform.
        let mut writer = BoundedWriter::new(&self.used_output, self.max_output_size);

        for strand in strands {
            match strand {
                P::Strand::Text(s) => writer
                    .write_str(s)
                    .map_err(|_| FormatError::TooMuchOutput)?,

                P::Strand::Expr(P::Expr {
                    alternates,
                    transform,
                }) => {
                    let Some(v) = self.eval_alts(alternates).await? else {
                        return Ok(serde_json::Value::Null);
                    };

                    v.format(*transform, &mut writer)?;
                }
            }
        }

        Ok(serde_json::Value::String(writer.finish()))
    }

    /// Evaluate each `chain` in turn until one succeeds (produces a non-`None` value).
    ///
    /// Returns the result from the first chain that produces a value, or `Ok(None)` if none do.
    /// Propagates any errors encountered during evaluation.
    async fn eval_alts(&self, alts: &'s [P::Chain<'s>]) -> Result<Option<Value<'s>>, FormatError> {
        for chain in alts {
            if let Some(v) = self.eval_chain(chain).await? {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }

    /// Evaluate a chain of field accesses against a root expression.
    ///
    /// If the chain does not have a root expression, the object being displayed is used as the
    /// root. The root is evaluated first, and then each successive accessor is applied to it.
    ///
    /// This function returns `Ok(Some(value))` if all nested accesses succeed. An access succeeds
    /// when the accessor evaluates to `Ok(Some(access))` and the part of the value it is
    /// describing exists.
    ///
    /// Any errors encountered during evaluation are propagated.
    async fn eval_chain(&self, chain: &'s P::Chain<'s>) -> Result<Option<Value<'s>>, FormatError> {
        use Accessor as A;
        use Value as V;

        // Evaluate the root (if it is provided) and the accessors, concurrently.
        let root: OptionFuture<_> = chain
            .root
            .as_ref()
            .map(|literal| self.eval_literal(literal))
            .into();

        let accessors = join_all(chain.accessors.iter().map(|a| self.eval_accessor(a)));
        let (root, accessors) = join!(root, accessors,);

        let mut root = match root {
            Some(Ok(Some(root))) => root,
            Some(Ok(None)) => return Ok(None),
            Some(Err(e)) => return Err(e),

            // If a root was not provided, the object being displayed is the root.
            None => V::Slice(self.root),
        };

        let Some(mut accessors) = accessors
            .into_iter()
            .collect::<Result<Option<Vec<_>>, _>>()?
        else {
            return Ok(None);
        };

        accessors.reverse();
        while let Some(accessor) = accessors.last() {
            match (root, accessor) {
                (V::Address(a), A::DFIndex(i)) => {
                    let bytes = bcs::to_bytes(&i)?;
                    let type_ = i.type_();
                    let df_id = derive_dynamic_field_id(a, &type_, &bytes)?;

                    let Some(field) = self
                        .store
                        .object(df_id.into())
                        .await
                        .map_err(|e| FormatError::Store(Arc::new(e)))?
                    else {
                        return Ok(None);
                    };

                    let field = match FieldVisitor::deserialize(field.bytes, field.layout) {
                        Ok(f) => f,
                        Err(DFV::Error::Visitor(e)) => return Err(FormatError::Visitor(e)),
                        Err(_) => return Ok(None),
                    };

                    if field.kind != DynamicFieldType::DynamicField {
                        return Ok(None);
                    }

                    accessors.pop();
                    root = V::Slice(Slice {
                        bytes: field.value_bytes,
                        layout: field.value_layout,
                    });
                }

                (V::Address(a), A::DOFIndex(i)) => {
                    let bytes = bcs::to_bytes(&i)?;
                    let type_ = DynamicFieldInfo::dynamic_object_field_wrapper(i.type_()).into();
                    let df_id = derive_dynamic_field_id(a, &type_, &bytes)?;

                    let Some(field) = self
                        .store
                        .object(df_id.into())
                        .await
                        .map_err(|e| FormatError::Store(Arc::new(e)))?
                    else {
                        return Ok(None);
                    };

                    let field = match FieldVisitor::deserialize(field.bytes, field.layout) {
                        Ok(f) => f,
                        Err(DFV::Error::Visitor(e)) => return Err(FormatError::Visitor(e)),
                        Err(_) => return Ok(None),
                    };

                    if field.kind != DynamicFieldType::DynamicObject {
                        return Ok(None);
                    }

                    let Ok(id) = AccountAddress::from_bytes(field.value_bytes) else {
                        return Ok(None);
                    };

                    let Some(slice) = self
                        .store
                        .object(id)
                        .await
                        .map_err(|e| FormatError::Store(Arc::new(e)))?
                    else {
                        return Ok(None);
                    };

                    accessors.pop();
                    root = V::Slice(slice);
                }

                // Fetch a single byte from a byte array, as long as the accessor evaluates to a
                // numeric index.
                (V::Bytes(bs), accessor) => {
                    let Some(&b) = accessor.as_numeric_index().and_then(|i| bs.get(i as usize))
                    else {
                        return Ok(None);
                    };

                    accessors.pop();
                    root = V::U8(b);
                }

                // `V::String` corresponds to `std::string::String` in Move, which contains a
                // single `bytes` field.
                (V::String(s), A::Field(f)) if *f == "bytes" => {
                    accessors.pop();
                    root = match s {
                        Cow::Borrowed(s) => V::Bytes(Cow::Borrowed(s.as_bytes())),
                        Cow::Owned(s) => V::Bytes(Cow::Owned(s.into_bytes())),
                    }
                }

                // Fetch an element from a vector literal, as long as the accessor evaluates to a
                // numeric index.
                (V::Vector(mut xs), accessor) => {
                    let Some(i) = accessor.as_numeric_index() else {
                        return Ok(None);
                    };

                    accessors.pop();
                    root = if i as usize >= xs.elements.len() {
                        return Ok(None);
                    } else {
                        xs.elements.swap_remove(i as usize)
                    };
                }

                // Fetch a field from a struct or enum literal.
                (V::Struct(Struct { fields, .. }) | V::Enum(Enum { fields, .. }), a) => {
                    let Some(value) = fields.get(a) else {
                        return Ok(None);
                    };

                    accessors.pop();
                    root = value;
                }

                // Use the remaining accessors to extract a value from a slice of a serialized
                // value. This can consume multiple accessors, but will pause if it encounters a
                // dynamic (object) field access.
                (V::Slice(slice), _) => {
                    let Some(value) = Extractor::deserialize_slice(slice, &mut accessors)? else {
                        return Ok(None);
                    };

                    root = value;
                }

                // Scalar values do not support accessors.
                (
                    V::Address(_)
                    | V::Bool(_)
                    | V::String(_)
                    | V::U8(_)
                    | V::U16(_)
                    | V::U32(_)
                    | V::U64(_)
                    | V::U128(_)
                    | V::U256(_),
                    _,
                ) => return Ok(None),
            }
        }

        // Detect if the value we sliced out was the serialized Move representation of `None` and
        // convert that to `Ok(None)`, otherwise return the extracted value.
        Ok(if root.is_none() { None } else { Some(root) })
    }

    /// Evaluates the contents of an accessor to a value.
    ///
    /// Returns `Ok(Some(value))` if the accessor evaluates to a value, otherwise it propagates
    /// errors or `None` values.
    async fn eval_accessor(
        &self,
        acc: &'s P::Accessor<'s>,
    ) -> Result<Option<Accessor<'s>>, FormatError> {
        use Accessor as VA;
        use P::Accessor as PA;

        Ok(match acc {
            PA::Field(f) => Some(VA::Field(f.as_str())),
            PA::Positional(i) => Some(VA::Positional(*i)),
            PA::Index(chain) => Box::pin(self.eval_chain(chain)).await?.map(VA::Index),
            PA::DFIndex(chain) => Box::pin(self.eval_chain(chain)).await?.map(VA::DFIndex),
            PA::DOFIndex(chain) => Box::pin(self.eval_chain(chain)).await?.map(VA::DOFIndex),
        })
    }

    /// Evaluate literals to values.
    ///
    /// Returns `Ok(Some(value))` if all parts of the literal evaluate to `Ok(Some(value))`,
    /// otherwise it propagates errors or `None` values.
    async fn eval_literal(
        &self,
        lit: &'s P::Literal<'s>,
    ) -> Result<Option<Value<'s>>, FormatError> {
        use P::Literal as L;
        use Value as V;

        Ok(match lit {
            L::Address(a) => Some(V::Address(*a)),
            L::Bool(b) => Some(V::Bool(*b)),
            L::U8(n) => Some(V::U8(*n)),
            L::U16(n) => Some(V::U16(*n)),
            L::U32(n) => Some(V::U32(*n)),
            L::U64(n) => Some(V::U64(*n)),
            L::U128(n) => Some(V::U128(*n)),
            L::U256(n) => Some(V::U256(*n)),
            L::ByteArray(bs) => Some(V::Bytes(bs.into())),
            L::String(s) => Some(V::String(s.clone())),

            L::Vector(v) => self.eval_chains(&v.elements).await?.map(|elements| {
                V::Vector(Vector {
                    type_: v.type_.as_ref(),
                    elements,
                })
            }),

            L::Struct(s) => self.eval_fields(&s.fields).await?.map(|fields| {
                V::Struct(Struct {
                    type_: &s.type_,
                    fields,
                })
            }),

            L::Enum(e) => self.eval_fields(&e.fields).await?.map(|fields| {
                V::Enum(Enum {
                    type_: &e.type_,
                    variant_name: e.variant_name,
                    variant_index: e.variant_index,
                    fields,
                })
            }),
        })
    }

    /// Evaluate the fields of a struct or enum literal, concurrently.
    ///
    /// Returns `Ok(Some(fields))` if all the field values evaluate to `Ok(Some(value))`, otherwise
    /// it propagates errors or `None` values.
    async fn eval_fields(
        &self,
        field: &'s P::Fields<'s>,
    ) -> Result<Option<Fields<'s>>, FormatError> {
        Ok(match field {
            P::Fields::Positional(fs) => self.eval_chains(fs).await?.map(Fields::Positional),
            P::Fields::Named(fs) => self
                .eval_chains(fs.iter().map(|(_, f)| f))
                .await?
                .map(|vs| Fields::Named(fs.iter().map(|(n, _)| *n).zip(vs).collect())),
        })
    }

    /// Evaluate multiple chains concurrently.
    ///
    /// If all chains evaluate to `Ok(Some(value))`, returns `Some(vec![value, ...])`, otherwise it
    /// propagates errors or `None` values.
    async fn eval_chains(
        &self,
        chains: impl IntoIterator<Item = &'s P::Chain<'s>>,
    ) -> Result<Option<Vec<Value<'s>>>, FormatError> {
        let values = chains
            .into_iter()
            .map(|chain| Box::pin(self.eval_chain(chain)));

        join_all(values).await.into_iter().collect()
    }
}
