// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Object};
use std::{collections::BTreeSet, sync::Arc};

use crate::{error::RpcError, scope::Scope, task::watermark::Watermarks};

use super::checkpoint::Checkpoint;

/// Key for querying checkpoint range availability by GraphQL type, field, and filters.
///
/// Falls back to type-level availability when field or filters are omitted.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct AvailableRangeKey {
    /// GraphQL type name
    pub(crate) type_: String,
    /// Specific field within the type
    pub(crate) field: Option<String>,
    /// Filter names for filtered queries
    pub(crate) filters: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct AvailableRange {
    pub scope: Scope,
    pub first: u64,
}

/// Checkpoint range for which data is available.
#[Object]
impl AvailableRange {
    /// Inclusive lower checkpoint for which data is available.
    async fn first(&self) -> Result<Option<Checkpoint>, RpcError> {
        Ok(Checkpoint::with_sequence_number(
            self.scope.clone(),
            Some(self.first),
        ))
    }

    /// Inclusive upper checkpoint for which data is available.
    async fn last(&self) -> Result<Option<Checkpoint>, RpcError> {
        Ok(Checkpoint::with_sequence_number(self.scope.clone(), None))
    }
}

/// Expands a series of type/field/filter patterns to generate the collect_pipelines function and accompanying tests to
/// validate that arguments of the macro invocation exist in the GraphQL schema registry.
///
/// # Generated Functions and Tests
///
/// - `collect_pipelines`: resolves type/field/filter combinations to the indexer pipelines where data is available.
/// - `test_schema_inclusion`: Testing entry point that calls the other tests using a GraphQL extension.
/// - `test_implements_interface`: Tests if a type implements an interface, it should delegate to the correct type and fields in the macro invocation
/// - `test_macro_invocation_matches_schema`: Tests that the macro invocation matches the types and fields in the GraphQL schema.
/// - `test_registry_collect_pipelines_snapshot`: Generates a snapshot of all type.field (filter) -> pipeline mappings for regression testing and auditing.
macro_rules! collect_pipelines {
    (
        $($type:ident . [$($field:ident),*]
        $(=> $delegate_type:ident . $delegate_field:tt $( ( $(.., $f_to_add:literal)? ) )? )?
        $(|$pipes:ident, $filt:ident| $block:block)?
        ;
    )*) => {
        /// Populates `pipelines` with indexer pipeline names by matching against the collect_pipelines! macro configuration.
        fn collect_pipelines(
            type_: &str,
            field: Option<&str>,
            filters: &mut BTreeSet<String>,
            pipelines: &mut BTreeSet<String>,
        ) {
            match (type_, field) {
                $(
                    (stringify!($type), _field @ Some($(stringify!($field)) |*)) => {
                        $(
                            $($(
                                filters.insert($f_to_add.to_string());
                            )?)?
                            let delegate_field = if stringify!($delegate_field) == "*" {
                                _field
                            } else {
                                Some(stringify!($delegate_field))
                            };
                            collect_pipelines(stringify!($delegate_type), delegate_field, filters, pipelines);
                        )?
                        $(
                            let $filt = filters;
                            let $pipes = pipelines;
                            $block
                        )?
                    }
                )*
                _ => {}
            }
        }

        #[cfg(test)]
        mod field_piplines_tests {
            use crate::schema;
            use async_graphql::{
                extensions::{Extension, ExtensionContext, ExtensionFactory, NextRequest},
                registry::{MetaType, Registry},
                Response,
            };
            use std::{collections::BTreeSet, sync::Arc};

            // Collect macro expansion into a vector of tuples for testing.
            const TYPE_FIELD_DELEGATIONS: &[(&str, &[&str], Option<(&str, &str)>)] = &[
            $(
                (
                    stringify!($type),
                    &[$(stringify!($field)),*],
                    {
                        #[allow(unused_assignments)]
                        let _delegation: Option<(&str, &str)> = None;
                        $(
                            let _delegation = Some((stringify!($delegate_type), stringify!($delegate_field)));
                        )?
                        _delegation
                    }
                ),
                )*
            ];

            /// Validates that all types, fields in the macro exist in the GraphQL schema
            /// and outputs a snapshot of all type.field (filter) -> pipeline mappings for regression testing.
            ///
            /// Test to ensure the macro configuration stays in sync with the GraphQL schema. If a type, field, or
            /// filter is referenced in the macro but doesn't exist in the schema, this test will fail.
            #[tokio::test]
            async fn test_schema_inclusion() {
                struct SchemaValidationExtension;

                impl ExtensionFactory for SchemaValidationExtension {
                    fn create(&self) -> Arc<dyn Extension> {
                        Arc::new(SchemaValidationExtensionImpl)
                    }
                }

                struct SchemaValidationExtensionImpl;

                #[async_trait::async_trait]
                impl Extension for SchemaValidationExtensionImpl {
                    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
                        let registry = &ctx.schema_env.registry;

                        test_macro_invocation_matches_schema(registry);
                        test_implements_interface(registry);
                        test_registry_collect_pipelines_snapshot(registry);

                        next.run(ctx).await
                    }
                }

                let schema = schema().extension(SchemaValidationExtension).finish();
                let response = schema.execute("{ __typename }").await;
                assert!(response.errors.is_empty(), "Schema operation failed: {:?}", response.errors);
            }

            /// If a type implements an interface, every field in the interface is delegated
            /// to that interface in the macro invocation.
            /// ex. CoinMetadata.[balance] => IMoveObject.balance();  in the macro invocation should throw an error because
            ///     Type 'CoinMetadata' field 'balance' should delegate to interface 'IAddressable' but delegates to 'IMoveObject'
            fn test_implements_interface(registry: &Registry) {
                use std::collections::HashMap;

                let mut type_delegations: HashMap<String, HashMap<String, (String, String)>> = HashMap::new();

                for (type_name, fields, delegation) in TYPE_FIELD_DELEGATIONS {
                    if let Some((delegate_type, delegate_field)) = delegation {
                        let type_map = type_delegations.entry(type_name.to_string()).or_default();

                        for field in *fields {
                            let resolved_delegate_field = if *delegate_field == "*" {
                                field.to_string()
                            } else {
                                delegate_field.to_string()
                            };

                            type_map.insert(
                                field.to_string(),
                                (delegate_type.to_string(), resolved_delegate_field)
                            );
                        }
                    }
                }

                for (interface_name, meta_type) in registry.types.iter() {
                    let (possible_types, interface_fields) = match meta_type {
                        MetaType::Interface { possible_types, fields, .. } => (possible_types, fields),
                        _ => continue,
                    };

                    for type_name in possible_types {
                        let type_dels = type_delegations.get(type_name.as_str());

                        for (interface_field_name, _) in interface_fields {
                            if let Some(delegations) = type_dels {
                                if let Some((delegate_type, delegate_field)) = delegations.get(interface_field_name) {
                                    assert_eq!(
                                        delegate_type, interface_name,
                                        "Type '{}' field '{}' should delegate to interface '{}' but delegates to '{}'",
                                        type_name, interface_field_name, interface_name, delegate_type
                                    );
                                    assert_eq!(
                                        delegate_field, interface_field_name,
                                        "Type '{}' field '{}' should delegate to interface field '{}' but delegates to '{}'",
                                        type_name, interface_field_name, interface_field_name, delegate_field
                                    );
                                }
                            }
                        }
                    }
                }
            }

            /// Validates that the macro invocation matches the schema. This is to catch any typos in types or fields in the macro invocation.
            fn test_macro_invocation_matches_schema(registry: &Registry) {
                let mut type_field_filters = Vec::new();
                $(
                    let type_name = stringify!($type);
                    let field_names = vec![$(stringify!($field)),*];
                    type_field_filters.push((type_name, field_names));
                )*

                for (type_name, field_names, _) in TYPE_FIELD_DELEGATIONS {
                    let fields = match registry.types.get(*type_name) {
                        Some(MetaType::Object { fields, .. } | MetaType::Interface { fields, .. }) => fields,
                        Some(_) => panic!("Type '{}' is not an Object or Interface type", type_name),
                        None => panic!("Type '{}' not found in schema registry", type_name),
                    };
                    for field_name in *field_names {
                        fields.get(*field_name)
                            .unwrap_or_else(|| panic!("Field '{}' not found in type '{}'", field_name, type_name));
                    }
                }
            }

            /// Calls collect_pipeline on types, fields, and filters in the schema registry and
            /// stores the input and output in a snapshot for regression testing and auditing.
            fn test_registry_collect_pipelines_snapshot(registry: &Registry) -> String {
                let mut output = String::new();

                for (type_name, meta_type) in registry.types.iter() {
                    if type_name.starts_with("__") {
                        continue;
                    }

                    let fields = match meta_type {
                        MetaType::Object { fields, .. } | MetaType::Interface { fields, .. } => fields,
                        _ => continue,
                    };

                    for (field_name, meta_field) in fields.iter() {
                        let filter_fields: Vec<String> = meta_field.args.get("filter")
                            .and_then(|arg| registry.types.get(&arg.ty))
                            .and_then(|t| match t {
                                MetaType::InputObject { input_fields, .. } => Some(input_fields.keys().cloned().collect()),
                                _ => None,
                            })
                            .unwrap_or_else(Vec::new);

                        for filter_field in std::iter::once(None).chain(filter_fields.iter().map(|f| Some(f.as_str()))) {
                            let mut filters = filter_field.iter().map(|s| s.to_string()).collect();
                            let mut pipelines = BTreeSet::new();
                            super::collect_pipelines(type_name, Some(field_name), &mut filters, &mut pipelines);

                            let filter_suffix = filter_field.map_or(String::new(), |f| format!(" (filter: {f})"));
                            let pipeline_strs: Vec<_> = pipelines.iter().map(|s| format!("\"{s}\"")).collect();
                            let pipeline_output = format!("{type_name}.{field_name}{filter_suffix}\n  => {{{}}}\n\n", pipeline_strs.join(", "));
                            output.push_str(&pipeline_output);
                        }
                    }
                }

                insta::assert_snapshot!(output);
                output
            }

        }
    };
}

// Pipeline mapping syntax:
// - `Type.[field1, field2, ...]`: GraphQL type and fields to match
// - `=> OtherType.*`: delegate to OtherType using the same field name
// - `=> OtherType.specificField()`: delegate to OtherType.specificField
// - `=> OtherType.field(.., "filterName")`: delegate and add filter constraint
// - `|pipelines, filters| { ... }`: block of statements operating on pipelines and filters to execute
collect_pipelines! {
    Address.[asObject] => IObject.objectAt();
    Address.[transactions] => Query.transactions(.., "affectedAddress");
    Address.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    Address.[defaultSuinsName] => IAddressable.defaultSuinsName;
    Address.[dynamicField, dynamicObjectField, multiGetDynamicFields, multiGetDynamicObjectFields] => IMoveObject.*;

    Checkpoint.[transactions] |pipelines, _filters| {
        pipelines.insert("cp_sequence_numbers".to_string());
        pipelines.insert("tx_digests".to_string());
    };

    CoinMetadata.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    CoinMetadata.[dynamicField, dynamicObjectField, multiGetDynamicFields, multiGetDynamicObjectFields] => IMoveObject.*;
    CoinMetadata.[dynamicFields] => IMoveObject.dynamicFields();
    CoinMetadata.[receivedTransactions] => IObject.receivedTransactions();
    CoinMetadata.[objectAt, objectVersionsAfter, objectVersionsBefore] => IObject.*;
    CoinMetadata.[supply] |pipelines, _filters| {
        pipelines.insert("consistent".to_string());
    };

    Epoch.[checkpoints] |pipelines, _filters| {
        pipelines.insert("cp_sequence_numbers".to_string());
    };
    Epoch.[coinDenyList] |pipelines, _filters| {
        pipelines.insert("obj_versions".to_string());
    };

    Event.[contents, eventBcs, sender, sequenceNumber, timestamp, transaction, transactionModule] |pipelines, _filters| {
        pipelines.insert("ev_struct_inst".to_string());
        pipelines.insert("tx_digests".to_string());
    };

    IAddressable.[balance, balances, multiGetBalances, objects] |pipelines, _filters| {
        pipelines.insert("consistent".to_string());
    };
    IAddressable.[defaultSuinsName] |pipelines, _filters| {
        pipelines.insert("obj_versions".to_string());
    };
    IMoveObject.[dynamicFields] |pipelines, _filters| {
        pipelines.insert("consistent".to_string());
    };
    IMoveObject.[dynamicField, dynamicObjectField, multiGetDynamicFields, multiGetDynamicObjectFields] |pipelines, _filters| {
        pipelines.insert("obj_versions".to_string());
    };

    IObject.[receivedTransactions] => Query.transactions(.., "affectedAddress");
    IObject.[objectAt, objectVersionsAfter, objectVersionsBefore] |pipelines, _filters| {
        pipelines.insert("obj_versions".to_string());
    };

    Object.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    Object.[defaultSuinsName] => IAddressable.defaultSuinsName();
    Object.[dynamicField, dynamicObjectField, multiGetDynamicFields, multiGetDynamicObjectFields] => IMoveObject.*;
    Object.[objectAt, objectVersionsAfter, objectVersionsBefore] => IObject.*;
    Object.[receivedTransactions] => IObject.receivedTransactions();

    MovePackage.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    MovePackage.[defaultSuinsName] => IAddressable.defaultSuinsName();
    MovePackage.[objectAt, objectVersionsAfter, objectVersionsBefore] => IObject.*;
    MovePackage.[receivedTransactions] => IObject.receivedTransactions();


    Query.[checkpoints] |pipelines, _filters| {
        pipelines.insert("cp_sequence_numbers".to_string());
    };
    Query.[coinMetadata] |pipelines, _filters| {
        pipelines.insert("consistent".to_string());
        pipelines.insert("obj_versions".to_string());
    };
    Query.[events] |pipelines, filters| {
        pipelines.insert("tx_digests".to_string());
        if filters.contains("module") {
            pipelines.insert("ev_emit_mod".to_string());
        } else {
            pipelines.insert("ev_struct_inst".to_string());
        }
    };
    Query.[object] |pipelines, filters| {
        if !filters.contains("version") {
            pipelines.insert("obj_versions".to_string());
        }
    };
    Query.[objects] |pipelines, _filters| {
        pipelines.insert("consistent".to_string());
    };
    Query.[objectVersions] |pipelines, _filters| {
        pipelines.insert("obj_versions".to_string());
    };
    Query.[transactions] |pipelines, filters| {
        pipelines.insert("cp_sequence_numbers".to_string());
        pipelines.insert("tx_digests".to_string());
        if filters.contains("function") {
            pipelines.insert("tx_calls".to_string());
        } else if filters.contains("affectedAddress") {
            pipelines.insert("tx_affected_addresses".to_string());
        } else if filters.contains("affectedObject") {
            pipelines.insert("tx_affected_objects".to_string());
        } else if filters.contains("sentAddress") {
            pipelines.insert("tx_affected_addresses".to_string());
        } else if filters.contains("kind") {
            pipelines.insert("tx_kinds".to_string());
        }
    };

    TransactionEffects.[balanceChanges] |pipelines, _filters| {
        pipelines.insert("tx_balance_changes".to_string());
        pipelines.insert("tx_digests".to_string());
    };
}

impl AvailableRange {
    /// Computes the available checkpoint range for a GraphQL query.
    ///
    /// The first checkpoint is the max reader_lo of the pipelines match by the AvailableRangeKey.
    /// The last checkpoint is scope.checkpoint_viewed_at.
    pub(crate) fn new(
        ctx: &Context<'_>,
        scope: &Scope,
        retention_key: AvailableRangeKey,
    ) -> Result<Self, RpcError> {
        let watermarks: &Arc<Watermarks> = ctx.data()?;
        let (mut pipelines, mut filters) = (
            BTreeSet::new(),
            BTreeSet::from_iter(retention_key.filters.unwrap_or_default()),
        );

        collect_pipelines(
            &retention_key.type_,
            retention_key.field.as_deref(),
            &mut filters,
            &mut pipelines,
        );

        let first = pipelines.iter().try_fold(0, |acc, pipeline| {
            watermarks
                .pipeline_lo_watermark(pipeline)
                .map(|wm| acc.max(wm.checkpoint()))
        })?;

        Ok(Self {
            scope: scope.clone(),
            first,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn test_collect_pipelines(
        type_: &str,
        field: Option<&str>,
        mut filters: BTreeSet<String>,
    ) -> BTreeSet<String> {
        let mut pipelines = BTreeSet::new();
        collect_pipelines(type_, field, &mut filters, &mut pipelines);
        pipelines
    }

    #[test]
    fn test_catch_all() {
        let invalid = test_collect_pipelines("UnknownType", Some("field"), BTreeSet::new());
        assert!(invalid.is_empty());
        let valid = test_collect_pipelines("Address", Some("digests"), BTreeSet::new());
        assert!(valid.is_empty());
    }
}
