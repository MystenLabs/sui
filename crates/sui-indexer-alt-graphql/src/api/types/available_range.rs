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

macro_rules! field_pipelines {
    ($($ty:ident . field @ $($field:ident) * {
        $(filters.insert($filter:literal);)?
        $(delegate => $delegate_ty:ident $(. $delegate_field:ident)?;)?
        $(pipelines.insert($pipeline:literal);)*
        $(if $if_filter:literal => $if_pipeline:literal
            $(
                else if $elif_filter:literal => $elif_pipeline:literal
            )*
            $(
                else => $else_pipeline:literal
            )?)?
    });* $(;)?) => {

        /// Resolves GraphQL type/field queries to their required watermark pipelines.
        ///
        /// Watermark pipelines track data availability ranges. This function gathers
        /// pipelines used to compute the available checkpoint range for the given type, field and filers.
        fn collect_pipelines(
            type_: &str,
            field: Option<&str>,
            filters: &mut BTreeSet<String>,
            pipelines: &mut BTreeSet<String>,
        ) {
            match (type_, field) {
                $(
                    // Match on the type and field
                    (stringify!($ty), _field @ Some($(stringify!($field)) |*)) => {
                        // Insert any filters that are intersected based on the type and field
                        $(
                            filters.insert($filter.to_string());
                        )?
                        // Delegate to another type/field:
                        // - `delegate => Type.field` uses the specified field
                        // - `delegate => Type` uses the matched field (_field)
                        $(
                            let delegate_field = None$(.or(Some(stringify!($delegate_field))))?;
                            collect_pipelines(
                                stringify!($delegate_ty),
                                delegate_field.or(_field),
                                filters,
                                pipelines
                            );
                        )?
                        // Insert any base pipelines that are included by default
                        $(
                            pipelines.insert($pipeline.to_string());
                        )*
                        // Conditionally add pipeline based on filter presence
                        $(
                            if filters.contains($if_filter) {
                                pipelines.insert($if_pipeline.to_string());
                            }
                            $(
                                else if filters.contains($elif_filter) {
                                    pipelines.insert($elif_pipeline.to_string());
                                }
                            )*
                            $(
                                else {
                                    pipelines.insert($else_pipeline.to_string());
                                }
                            )?
                        )?
                    }
                )*
                _ => {}
            }
        }

        #[cfg(test)]
        mod field_piplines_tests {
            use async_graphql::{Context, Object, MergedObject, registry::MetaType};

            #[derive(Default)]
            struct SchemaValidator;

            #[Object]
            impl SchemaValidator {
                async fn validate_fields(&self, ctx: &Context<'_>) -> bool {
                    let registry = &ctx.schema_env.registry;

                    $(
                        // Validate that each type/field pair declared in field_pipelines! exists in the GraphQL schema
                        let type_name = stringify!($ty);
                        let meta_type = registry
                            .types
                            .get(type_name)
                            .unwrap_or_else(|| panic!("Type '{}' not found in schema registry", type_name));

                        let fields = match meta_type {
                                MetaType::Object { fields, .. } => fields,
                                MetaType::Interface { fields, .. } => fields,
                                _ => panic!("Type '{}' is not an Object or Interface type", type_name),
                            };

                        let field_names = vec![$(stringify!($field)),*];

                        for field_name in field_names {
                            assert!(
                                fields.contains_key(field_name),
                                "Field '{}.{}' not found in schema registry",
                                type_name,
                                field_name
                            );

                            let _meta_field = fields.get(field_name).unwrap();

                            // Validate that filter arguments referenced in field_pipelines! exist in the schema.
                            $(
                                let validate_filter = |filter_name: &str| {
                                    if let Some(MetaType::InputObject { input_fields, .. }) = _meta_field.args.get("filter")
                                        .and_then(|arg| registry.types.get(&arg.ty))
                                    {
                                        assert!(input_fields.contains_key(filter_name),
                                            "Filter '{}' not found in field '{}.{}'", filter_name, type_name, field_name);
                                    }
                                };
                                validate_filter($if_filter);
                                $(validate_filter($elif_filter);)*
                            )?
                        }
                    )*

                    true
                }
            }

            #[tokio::test]
            async fn test_schema_inclusion() {
                #[derive(MergedObject, Default)]
                struct TestQuery(crate::api::query::Query, SchemaValidator);
                let schema = async_graphql::Schema::build(
                    TestQuery::default(),
                    crate::api::mutation::Mutation,
                    async_graphql::EmptySubscription,
                )
                .register_output_type::<crate::api::types::address::IAddressable>()
                .register_output_type::<crate::api::types::move_datatype::IMoveDatatype>()
                .register_output_type::<crate::api::types::move_object::IMoveObject>()
                .register_output_type::<crate::api::types::object::IObject>()
                .finish();

                let response = schema.execute("{ validateFields }").await;
                assert!(response.errors.is_empty(), "Schema validation failed: {:?}", response.errors);
            }

            pub(crate) fn type_field_filter_to_pipeline_snapshot() -> String {
                use std::fmt::Write;
                use std::collections::BTreeSet;
                let mut output = String::new();

                $(
                    let type_name = stringify!($ty);
                    let field_names = vec![$(stringify!($field)),*];

                    for field_name in field_names {
                        // Collect all filters that might be used with this field
                        #[allow(unused_mut)]
                        let mut all_filters: BTreeSet<String> = BTreeSet::new();
                        $(
                            all_filters.insert($filter.to_string());
                        )?
                        $(
                            all_filters.insert($if_filter.to_string());
                            $(
                                all_filters.insert($elif_filter.to_string());
                            )*
                        )?

                        // Helper closure to write pipelines for a given filter set
                        let mut write_pipelines = |filter_opt: Option<&str>| {
                            let (mut pipelines, mut filters) = (BTreeSet::new(), BTreeSet::new());
                            if let Some(f) = filter_opt {
                                filters.insert(f.to_string());
                            }
                            super::collect_pipelines(type_name, Some(field_name), &mut filters, &mut pipelines);

                            match filter_opt {
                                Some(f) => writeln!(output, "{}.{} (filter: {})", type_name, field_name, f),
                                None => writeln!(output, "{}.{}", type_name, field_name),
                            }.unwrap();
                            if !pipelines.is_empty() {
                                writeln!(output, "  pipelines: {:?}", pipelines).unwrap();
                            }
                            writeln!(output).unwrap();
                        };

                        // Write pipelines for the type.field without any filters
                        write_pipelines(None);
                        // Write pipelines for the type.field with each filter
                        all_filters.iter().for_each(|f| write_pipelines(Some(f)));
                    }
                )*

                output
            }

            #[test]
            fn test_schema_pipeline_export() {
                let snapshot = type_field_filter_to_pipeline_snapshot();
                insta::assert_snapshot!(snapshot);
            }
        }
    };
}

// Pipeline mapping syntax:
// - `Type.field @ field1 field2`: GraphQL type and fields to match
// - `delegate => Type.field`: inherit pipelines from Type.field
// - `delegate => Type`: inherit pipelines from Type using the matched field
// - `filters.insert("name")`: add filter constraint before delegating
// - `pipelines.insert("name")`: unconditionally required pipelines
// - `if "filter" => "pipeline"`: filter-conditional pipeline selection
field_pipelines! {
    Address.field @ asObject {
        delegate => IObject.objectAt;
    };
    Address.field @ transactions {
        filters.insert("affectedAddress");
        delegate => Query.transactions;
    };
    Address.field @ balance balances multiGetBalances objects {
        delegate => IAddressable;
    };
    Address.field @ defaultSuinsName {
        delegate => IAddressable.defaultSuinsName;
    };
    Address.field @ dynamicField dynamicObjectField multiGetDynamicFields multiGetDynamicObjectFields {
        delegate => IMoveObject;
    };

    Checkpoint.field @ transactions {
        filters.insert("atCheckpoint");
        delegate => Query.transactions;
    };

    CoinMetadata.field @ balance balances multiGetBalances objects{
        delegate => IAddressable;
    };
    CoinMetadata.field @ dynamicField dynamicObjectField multiGetDynamicFields multiGetDynamicObjectFields {
        delegate => IMoveObject;
    };
    CoinMetadata.field @ dynamicFields {
        delegate => IMoveObject.dynamicFields;
    };
    CoinMetadata.field @ receivedTransactions {
        delegate => IObject.receivedTransactions;
    };
    CoinMetadata.field @ objectAt objectVersionsAfter objectVersionsBefore {
        delegate => IObject;
    };
    CoinMetadata.field @ supply {
        pipelines.insert("consistent");
    };

    Epoch.field @ checkpoints {
        delegate => Query.checkpoints;
    };
    Epoch.field @ coinDenyList {
        pipelines.insert("obj_versions");
    };

    Event.field @ contents eventBcs sender sequenceNumber timestamp transaction transactionModule {
        delegate => Query.events;
    };

    IAddressable.field @ balance balances multiGetBalances objects {
        pipelines.insert("consistent");
    };
    IAddressable.field @ defaultSuinsName {
        pipelines.insert("obj_versions");
    };

    IMoveObject.field @ dynamicFields {
        pipelines.insert("consistent");
    };
    IMoveObject.field @ dynamicField dynamicObjectField multiGetDynamicFields multiGetDynamicObjectFields {
        pipelines.insert("obj_versions");
    };

    IObject.field @ receivedTransactions {
        filters.insert("affectedAddress");
        delegate => Query.transactions;
    };
    IObject.field @ objectAt objectVersionsAfter objectVersionsBefore {
        pipelines.insert("obj_versions");
    };

    Object.field @ balance balances multiGetBalances objects {
        delegate => IAddressable;
    };
    Object.field @ defaultSuinsName {
        delegate => IAddressable.defaultSuinsName;
    };
    Object.field @ dynamicField dynamicObjectField multiGetDynamicFields multiGetDynamicObjectFields {
        delegate => IMoveObject;
    };
    Object.field @ objectAt objectVersionsAfter objectVersionsBefore {
        delegate => IObject;
    };
    Object.field @ receivedTransactions {
        delegate => IObject.receivedTransactions;
    };

    MovePackage.field @ balance balances multiGetBalances objects {
        delegate => IAddressable;
    };
    MovePackage.field @ defaultSuinsName {
        delegate => IAddressable.defaultSuinsName;
    };
    MovePackage.field @ objectAt objectVersionsAfter objectVersionsBefore {
        delegate => IObject;
    };
    MovePackage.field @receivedTransactions {
        delegate => IObject.receivedTransactions;
    };

    Query.field @checkpoints {
        pipelines.insert("cp_sequence_numbers");
    };
    Query.field @coinMetadata {
        pipelines.insert("consistent");
        pipelines.insert("obj_versions");
    };
    Query.field @events {
        pipelines.insert("tx_digests");
        if "module" => "ev_emit_mod"
        else => "ev_struct_inst"
    };
    Query.field @object {
        if "version" => ""
        else => "obj_versions"
    };
    Query.field @objects {
        pipelines.insert("consistent");
    };
    Query.field @objectVersions {
        pipelines.insert("obj_versions");
    };
    Query.field @ transactions {
        pipelines.insert("tx_digests");
        pipelines.insert("cp_sequence_numbers");
        if "function" => "tx_calls"
        else if "affectedAddress" => "tx_affected_addresses"
        else if "affectedObject" => "tx_affected_objects"
        else if "sentAddress" => "tx_affected_addresses"
        else if "kind" => "tx_kinds"
    };

    TransactionEffects.field @ balanceChanges {
        pipelines.insert("tx_balance_changes");
        pipelines.insert("tx_digests");
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
