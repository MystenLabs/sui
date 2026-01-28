// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Cow;
use std::mem;
use std::sync::Arc;

use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use futures::future::OptionFuture;
use futures::future::join_all;
use futures::join;
use move_core_types::account_address::AccountAddress;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::dynamic_field::visitor as DFV;
use sui_types::dynamic_field::visitor::FieldVisitor;

use crate::v2::error::FormatError;
use crate::v2::parser as P;
use crate::v2::value as V;
use crate::v2::visitor::extractor::Extractor;

/// The interpreter is responsible for evaluating expressions inside format strings into values.
pub struct Interpreter<S: V::Store> {
    store: S,

    /// Cache of the objects that have been fetched so far. This cache is never evicted -- it is
    /// used to keep objects alive for the lifetime of the interpreter.
    cache: DashMap<AccountAddress, Option<Arc<V::OwnedSlice>>>,

    root: V::OwnedSlice,
}

impl<S: V::Store> Interpreter<S> {
    /// Create a new interpreter instance. `root` is the contents (bytes and layout) of an object
    /// that acts as the root of all field accesses. `store` is used to fetch additional objects as
    /// needed.
    pub fn new(root: V::OwnedSlice, store: S) -> Self {
        Self {
            store,
            cache: DashMap::new(),
            root,
        }
    }

    /// Entrypoint to evaluate a single format string, represented as a sequence of its strands.
    /// Returns evaluated strands that can then be formatted.
    pub(crate) async fn eval_strands<'s>(
        &'s self,
        strands: &'s [P::Strand<'s>],
    ) -> Result<Option<Vec<V::Strand<'s>>>, FormatError> {
        join_all(strands.iter().map(|strand| async move {
            match strand {
                P::Strand::Text(s) => Ok(Some(V::Strand::Text(s))),
                P::Strand::Expr(P::Expr {
                    offset,
                    alternates,
                    transform,
                }) => {
                    let transform = transform.unwrap_or_default();
                    Ok(self
                        .eval_alts(alternates)
                        .await?
                        .map(move |value| V::Strand::Value {
                            value,
                            transform,
                            offset: *offset,
                        }))
                }
            }
        }))
        .await
        .into_iter()
        .collect()
    }

    /// Evaluate each `chain` in turn until one succeeds (produces a non-`None` value).
    ///
    /// Returns the result from the first chain that produces a value, or `Ok(None)` if none do.
    /// Propagates any errors encountered during evaluation.
    async fn eval_alts<'s>(
        &'s self,
        alts: &'s [P::Chain<'s>],
    ) -> Result<Option<V::Value<'s>>, FormatError> {
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
    pub(crate) async fn eval_chain<'s>(
        &'s self,
        chain: &'s P::Chain<'s>,
    ) -> Result<Option<V::Value<'s>>, FormatError> {
        use V::Accessor as A;
        use V::Value as VV;

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
            None => VV::Slice(self.root.as_slice()),
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
                (VV::Address(a), A::DFIndex(i)) => {
                    let df_id = i.derive_dynamic_field_id(a)?.into();
                    let Some(slice) = self.object(df_id).await? else {
                        return Ok(None);
                    };

                    let field = match FieldVisitor::deserialize(slice.bytes, slice.layout) {
                        Ok(f) => f,
                        Err(DFV::Error::Visitor(e)) => return Err(FormatError::Visitor(e)),
                        Err(_) => return Ok(None),
                    };

                    if field.kind != DynamicFieldType::DynamicField {
                        return Ok(None);
                    }

                    accessors.pop();
                    root = VV::Slice(V::Slice {
                        bytes: field.value_bytes,
                        layout: field.value_layout,
                    });
                }

                (VV::Address(a), A::DOFIndex(i)) => {
                    let df_id = i.derive_dynamic_object_field_id(a)?.into();
                    let Some(slice) = self.object(df_id).await? else {
                        return Ok(None);
                    };

                    let field = match FieldVisitor::deserialize(slice.bytes, slice.layout) {
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

                    let Some(value_slice) = self.object(id).await? else {
                        return Ok(None);
                    };

                    accessors.pop();
                    root = VV::Slice(value_slice);
                }

                // Fetch a single byte from a byte array, as long as the accessor evaluates to a
                // numeric index.
                (VV::Bytes(bs), accessor) => {
                    let Some(&b) = accessor.as_numeric_index().and_then(|i| bs.get(i as usize))
                    else {
                        return Ok(None);
                    };

                    accessors.pop();
                    root = VV::U8(b);
                }

                // `V::String` corresponds to `std::string::String` in Move, which contains a
                // single `bytes` field.
                (VV::String(s), A::Field(f)) if *f == "bytes" => {
                    accessors.pop();
                    root = VV::Bytes(s)
                }

                // Fetch an element from a vector literal, as long as the accessor evaluates to a
                // numeric index.
                (VV::Vector(mut xs), accessor) => {
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
                (VV::Struct(V::Struct { fields, .. }) | VV::Enum(V::Enum { fields, .. }), a) => {
                    let Some(value) = fields.get(a) else {
                        return Ok(None);
                    };

                    accessors.pop();
                    root = value;
                }

                // Use the remaining accessors to extract a value from a slice of a serialized
                // value. This can consume multiple accessors, but will pause if it encounters a
                // dynamic (object) field access.
                (VV::Slice(slice), _) => {
                    let Some(value) = Extractor::deserialize_slice(slice, &mut accessors)? else {
                        return Ok(None);
                    };

                    root = value;
                }

                // Scalar values do not support accessors.
                (
                    VV::Address(_)
                    | VV::Bool(_)
                    | VV::String(_)
                    | VV::U8(_)
                    | VV::U16(_)
                    | VV::U32(_)
                    | VV::U64(_)
                    | VV::U128(_)
                    | VV::U256(_),
                    _,
                ) => return Ok(None),
            }
        }

        Ok(Some(root))
    }

    /// Evaluates the contents of an accessor to a value.
    ///
    /// Returns `Ok(Some(value))` if the accessor evaluates to a value, otherwise it propagates
    /// errors or `None` values.
    async fn eval_accessor<'s>(
        &'s self,
        acc: &'s P::Accessor<'s>,
    ) -> Result<Option<V::Accessor<'s>>, FormatError> {
        use P::Accessor as PA;
        use V::Accessor as VA;

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
    pub(crate) async fn eval_literal<'s>(
        &'s self,
        lit: &'s P::Literal<'s>,
    ) -> Result<Option<V::Value<'s>>, FormatError> {
        use P::Literal as L;
        use V::Value as VV;

        Ok(match lit {
            L::Address(a) => Some(VV::Address(*a)),
            L::Bool(b) => Some(VV::Bool(*b)),
            L::U8(n) => Some(VV::U8(*n)),
            L::U16(n) => Some(VV::U16(*n)),
            L::U32(n) => Some(VV::U32(*n)),
            L::U64(n) => Some(VV::U64(*n)),
            L::U128(n) => Some(VV::U128(*n)),
            L::U256(n) => Some(VV::U256(*n)),
            L::ByteArray(bs) => Some(VV::Bytes(bs.into())),

            L::String(s) => match s.clone() {
                Cow::Borrowed(s) => Some(VV::String(Cow::Borrowed(s.as_bytes()))),
                Cow::Owned(s) => Some(VV::String(Cow::Owned(s.into_bytes()))),
            },

            L::Vector(v) => self.eval_chains(&v.elements).await.and_then(|elements| {
                let Some(elements) = elements else {
                    return Ok(None);
                };

                // Evaluate the vector's element type and check that it is consistent across all
                // elements.
                let type_ = if let Some(explicit) = &v.type_ {
                    Cow::Borrowed(explicit)
                } else if let Some(first) = elements.first() {
                    Cow::Owned(first.type_())
                } else {
                    return Err(FormatError::VectorNoType);
                };

                for e in &elements {
                    let element_type = e.type_();
                    if element_type != *type_ {
                        return Err(FormatError::VectorTypeMismatch {
                            offset: v.offset,
                            this: type_.into_owned(),
                            that: element_type,
                        });
                    }
                }

                Ok(Some(VV::Vector(V::Vector { type_, elements })))
            })?,

            L::Struct(s) => self.eval_fields(&s.fields).await?.map(|fields| {
                VV::Struct(V::Struct {
                    type_: &s.type_,
                    fields,
                })
            }),

            L::Enum(e) => self.eval_fields(&e.fields).await?.map(|fields| {
                VV::Enum(V::Enum {
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
    async fn eval_fields<'s>(
        &'s self,
        field: &'s P::Fields<'s>,
    ) -> Result<Option<V::Fields<'s>>, FormatError> {
        Ok(match field {
            P::Fields::Positional(fs) => self.eval_chains(fs).await?.map(V::Fields::Positional),
            P::Fields::Named(fs) => self
                .eval_chains(fs.iter().map(|(_, f)| f))
                .await?
                .map(|vs| V::Fields::Named(fs.iter().map(|(n, _)| *n).zip(vs).collect())),
        })
    }

    /// Evaluate multiple chains concurrently.
    ///
    /// If all chains evaluate to `Ok(Some(value))`, returns `Some(vec![value, ...])`, otherwise it
    /// propagates errors or `None` values.
    async fn eval_chains<'s>(
        &'s self,
        chains: impl IntoIterator<Item = &'s P::Chain<'s>>,
    ) -> Result<Option<Vec<V::Value<'s>>>, FormatError> {
        let values = chains
            .into_iter()
            .map(|chain| Box::pin(self.eval_chain(chain)));

        join_all(values).await.into_iter().collect()
    }

    /// Fetch an object from the store, caching the result.
    ///
    /// Returns a `Slice<'s>` that borrows from the cached data. The returned slice is guaranteed
    /// to remain valid for the lifetime 's because the cache (and the Arc it contains) lives for
    /// the entire lifetime of the Interpreter.
    async fn object<'s>(&'s self, id: AccountAddress) -> Result<Option<V::Slice<'s>>, FormatError> {
        let entry = match self.cache.entry(id) {
            Entry::Occupied(entry) => entry,
            Entry::Vacant(entry) => entry.insert_entry(
                self.store
                    .object(id)
                    .await
                    .map_err(|e| FormatError::Store(Arc::new(e)))?
                    .map(Arc::new),
            ),
        };

        let Some(owned) = entry.get().as_ref() else {
            return Ok(None);
        };

        // SAFETY: Extending the lifetime of the slice from the reference (local to this
        // scope), to the lifetime of the interpreter.  This is safe because the reference is
        // pointing into an Arc that is owned by the interpreter.
        let slice = owned.as_slice();
        Ok(Some(unsafe {
            mem::transmute::<V::Slice<'_>, V::Slice<'s>>(slice)
        }))
    }
}
