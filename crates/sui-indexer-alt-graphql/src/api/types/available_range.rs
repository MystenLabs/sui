// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Object};
use std::{collections::BTreeSet, sync::Arc};

use crate::{error::RpcError, scope::Scope, task::watermark::Watermarks};

use super::checkpoint::Checkpoint;

/// Identifies a GraphQL query component that is used to determine the range of checkpoints for which data is available (for data that can be tied to a particular checkpoint).
///
/// Provides retention information for the type and optional field and filters. If field or filters are not provided we fall back to the available range for the type.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct AvailableRangeKey {
    /// The GraphQL type to check retention for
    pub(crate) type_: String,

    /// The specific field within the type to check retention for
    pub(crate) field: Option<String>,

    /// Optional filter to check retention for filtered queries
    pub(crate) filters: Option<Vec<String>>,
}

#[derive(Clone)]
pub(crate) struct AvailableRange {
    pub(crate) scope: Scope,
    pub(crate) first: u64,
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
        let filters = BTreeSet::from_iter(retention_key.filters.unwrap_or_default());
        let mut pipelines = BTreeSet::new();

        collect_pipelines(
            &retention_key.type_,
            retention_key.field.as_deref(),
            filters,
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

/// Expands a series of type/field/filter patterns to generate the collect_pipelines function and collects macro invocation data into
/// a HashMap of (Type, Field) to (DelegateType, DelegateField) for tests.
macro_rules! collect_pipelines {
    (
        $($type:ident . [$($field:ident),*]
        $(=> $delegate_type:ident . $delegate_field:tt $( ( $(.., $f_to_add:literal)? ) )? )?
        $(|$pipes:ident, $filt:ident| $block:block)?
        ;
    )*) => {
        /// Populates `pipelines` with pipeline names by matching the type, field, and filters to their dependent pipelines where data is available.
        /// The mapping is defined in the collect_pipelines! macro innvocation.
        fn collect_pipelines(
            type_: &str,
            field: Option<&str>,
            mut filters: BTreeSet<String>,
            pipelines: &mut BTreeSet<String>,
        ) {
            match (type_, field) {
                $(
                    (stringify!($type), _field @ Some($(stringify!($field)) |*)) => {
                        $(
                            $($(
                                filters.insert($f_to_add.to_string());
                            )?)?
                            let delegate_field = delegate!(_field, $delegate_field);
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

        /// Map from (Type, Field) to (DelegateType, DelegateField) for testing.
        /// Maps to ("", "") if the type/field is not delegated. The "*" wildcard is expanded.
        #[cfg(test)]
        static TYPE_FIELD_DELEGATIONS: std::sync::LazyLock<std::collections::HashMap<(&'static str, &'static str), (&'static str, &'static str)>> =
            std::sync::LazyLock::new(|| {
                let mut map = std::collections::HashMap::new();
                $(
                    let type_name = stringify!($type);
                    let fields = [$(stringify!($field),)*];
                    let delegate_type = stringify!($($delegate_type)?);
                    for field in fields {
                        let delegate_field = match stringify!($($delegate_field)?) {
                            "*" => field,
                            _ => stringify!($($delegate_field)?),
                        };
                        map.insert((type_name, field), (delegate_type, delegate_field));
                    }
                )*
                map
            });
    };
}

macro_rules! delegate {
    ($field:ident, *) => {
        $field
    };
    ($field:ident, $delegate:tt) => {
        Some(stringify!($delegate))
    };
}

// Pipeline mapping syntax:
// - `Type.[field1, field2, ...]`: GraphQL type and fields to match
// - `=> OtherType.*`: delegate to OtherType using the same field name
// - `=> OtherType.specificField()`: delegate to OtherType.specificField
// - `=> OtherType.field(.., "filterName")`: delegate and add filter constraint
// - `|pipelines, filters| { ... }`: block of statements operating on pipelines and filters to execute
collect_pipelines! {
    Address.[address] => IAddressable.*;
    Address.[asObject] => IObject.objectAt();
    Address.[transactions] => Query.transactions(.., "affectedAddress");
    Address.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    Address.[defaultSuinsName] => IAddressable.defaultSuinsName;
    Address.[dynamicField, dynamicFields, dynamicObjectField, multiGetDynamicFields, multiGetDynamicObjectFields] => IMoveObject.*;

    Checkpoint.[transactions] => Query.transactions(.., "atCheckpoint");

    CoinMetadata.[address] => IAddressable.*;
    CoinMetadata.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    CoinMetadata.[defaultSuinsName] => IAddressable.defaultSuinsName();
    CoinMetadata.[contents, hasPublicTransfer, moveObjectBcs] => IMoveObject.*;
    CoinMetadata.[dynamicField, dynamicObjectField, multiGetDynamicFields, multiGetDynamicObjectFields] => IMoveObject.*;
    CoinMetadata.[dynamicFields] => IMoveObject.dynamicFields();
    CoinMetadata.[objectAt, objectVersionsAfter, objectVersionsBefore] => IObject.*;
    CoinMetadata.[digest, objectBcs, owner, previousTransaction, storageRebate, version] => IObject.*;
    CoinMetadata.[receivedTransactions] => IObject.receivedTransactions();
    CoinMetadata.[supply] |pipelines, _filters| {
        pipelines.insert("consistent".to_string());
    };
    CoinMetadata.[supplyState] |pipelines, _filters| {
        pipelines.insert("consistent".to_string());
    };

    DynamicField.[address] => IAddressable.*;
    DynamicField.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    DynamicField.[defaultSuinsName] => IAddressable.defaultSuinsName();
    DynamicField.[contents, hasPublicTransfer, moveObjectBcs] => IMoveObject.*;
    DynamicField.[dynamicField, dynamicObjectField, multiGetDynamicFields, multiGetDynamicObjectFields] => IMoveObject.*;
    DynamicField.[dynamicFields] => IMoveObject.dynamicFields();
    DynamicField.[objectAt, objectVersionsAfter, objectVersionsBefore] => IObject.*;
    DynamicField.[digest, objectBcs, owner, previousTransaction, storageRebate, version] => IObject.*;
    DynamicField.[receivedTransactions] => IObject.receivedTransactions();

    Epoch.[checkpoints] |pipelines, _filters| {
        pipelines.insert("cp_sequence_numbers".to_string());
    };
    Epoch.[coinDenyList] |pipelines, _filters| {
        pipelines.insert("obj_versions".to_string());
    };
    Epoch.[transactions] => Query.transactions(.., "atCheckpoint");

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

    IMoveDatatype.[abilities, typeParameters] |pipelines, _filters| {
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

    MoveDatatype.[module, name] => IMoveDatatype.*;
    MoveDatatype.[abilities, typeParameters] => IMoveDatatype.*;
    MoveDatatype.[asMoveEnum, asMoveStruct] |pipelines, _filters| {
        pipelines.insert("obj_versions".to_string());
    };

    MoveEnum.[module, name] => IMoveDatatype.*;
    MoveEnum.[abilities, typeParameters] => IMoveDatatype.*;
    MoveEnum.[variants] |pipelines, _filters| {
        pipelines.insert("obj_versions".to_string());
    };

    MoveObject.[address] => IAddressable.*;
    MoveObject.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    MoveObject.[defaultSuinsName] => IAddressable.defaultSuinsName();
    MoveObject.[contents, hasPublicTransfer, moveObjectBcs] => IMoveObject.*;
    MoveObject.[dynamicField, dynamicObjectField, multiGetDynamicFields, multiGetDynamicObjectFields] => IMoveObject.*;
    MoveObject.[dynamicFields] => IMoveObject.dynamicFields();
    MoveObject.[objectAt, objectVersionsAfter, objectVersionsBefore] => IObject.*;
    MoveObject.[digest, objectBcs, owner, previousTransaction, storageRebate, version] => IObject.*;
    MoveObject.[receivedTransactions] => IObject.receivedTransactions();

    MovePackage.[address] => IAddressable.*;
    MovePackage.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    MovePackage.[defaultSuinsName] => IAddressable.defaultSuinsName();
    MovePackage.[objectAt, objectVersionsAfter, objectVersionsBefore] => IObject.*;
    MovePackage.[digest, objectBcs, owner, previousTransaction, storageRebate, version] => IObject.*;
    MovePackage.[receivedTransactions] => IObject.receivedTransactions();

    MoveStruct.[module, name] => IMoveDatatype.*;
    MoveStruct.[abilities, typeParameters] => IMoveDatatype.*;
    MoveStruct.[fields] |pipelines, _filters| {
        pipelines.insert("obj_versions".to_string());
    };

    Object.[address] => IAddressable.*;
    Object.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    Object.[defaultSuinsName] => IAddressable.defaultSuinsName();
    Object.[dynamicField, dynamicObjectField, multiGetDynamicFields, multiGetDynamicObjectFields] => IMoveObject.*;
    Object.[dynamicFields] => IMoveObject.dynamicFields();
    Object.[objectAt, objectVersionsAfter, objectVersionsBefore, version] => IObject.*;
    Object.[digest, objectBcs, owner, previousTransaction, storageRebate, version] => IObject.*;
    Object.[receivedTransactions] => IObject.receivedTransactions();

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
        if filters.contains("affectedAddress") {
            pipelines.insert("tx_affected_addresses".to_string());
        } else if filters.contains("function") {
            pipelines.insert("tx_calls".to_string());
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

    Validator.[address] => IAddressable.*;
    Validator.[balance, balances, multiGetBalances, objects] => IAddressable.*;
    Validator.[defaultSuinsName] => IAddressable.defaultSuinsName();
    Validator.[operationCap] |pipelines, _filters| {
        pipelines.insert("obj_versions".to_string());
    };
}

#[cfg(test)]
mod field_piplines_tests {
    use super::*;
    use crate::schema;
    use async_graphql::{
        Response,
        extensions::{Extension, ExtensionContext, ExtensionFactory, NextRequest},
        registry::{MetaType, MetaTypeName, Registry},
    };
    use std::{collections::BTreeSet, sync::Arc};

    fn test_collect_pipelines(
        type_: &str,
        field: Option<&str>,
        filters: BTreeSet<String>,
    ) -> BTreeSet<String> {
        let mut pipelines = BTreeSet::new();
        collect_pipelines(type_, field, filters, &mut pipelines);
        pipelines
    }

    #[test]
    fn test_catch_all() {
        let invalid = test_collect_pipelines("UnknownType", Some("field"), BTreeSet::new());
        assert!(invalid.is_empty());
        let valid = test_collect_pipelines("Address", Some("digests"), BTreeSet::new());
        assert!(valid.is_empty());
    }

    /// Helper function that runs a test function with access to the GraphQL schema registry.
    async fn with_registry(test_fn: fn(&Registry)) {
        struct TestExtension {
            test_fn: fn(&Registry),
        }

        impl ExtensionFactory for TestExtension {
            fn create(&self) -> Arc<dyn Extension> {
                Arc::new(TestExtensionImpl {
                    test_fn: self.test_fn,
                })
            }
        }

        struct TestExtensionImpl {
            test_fn: fn(&Registry),
        }

        #[async_trait::async_trait]
        impl Extension for TestExtensionImpl {
            async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
                (self.test_fn)(&ctx.schema_env.registry);
                next.run(ctx).await
            }
        }

        let schema = schema().extension(TestExtension { test_fn }).finish();
        let response = schema.execute("{ __typename }").await;
        assert!(
            response.errors.is_empty(),
            "Schema operation failed: {:?}",
            response.errors
        );
    }

    #[tokio::test]
    async fn test_macro_invocation_matches_schema() {
        with_registry(macro_invocation_matches_schema).await;
    }

    #[tokio::test]
    async fn test_type_delegation_matches_interface() {
        with_registry(type_delegation_matches_interface).await;
    }

    #[tokio::test]
    async fn test_registry_collect_pipelines_snapshot() {
        with_registry(registry_collect_pipelines_snapshot).await;
    }

    /// If a type implements an interface, every field in the interface is delegated to that interface in the macro invocation.
    /// ex. CoinMetadata.[balance] => IMoveObject.balance();  in the macro invocation should throw an error because
    ///     Type 'CoinMetadata' field 'balance' should delegate to interface 'IAddressable' but delegates to 'IMoveObject'
    fn type_delegation_matches_interface(registry: &Registry) {
        let type_delegations = &TYPE_FIELD_DELEGATIONS;

        for (interface_name, meta_type) in registry.types.iter() {
            let MetaType::Interface {
                possible_types,
                fields,
                ..
            } = meta_type
            else {
                continue;
            };
            for type_name in possible_types {
                for (interface_field_name, _) in fields {
                    let Some((delegate_type, delegate_field)) =
                        type_delegations.get(&(type_name.as_str(), interface_field_name.as_str()))
                    else {
                        panic!(
                            "Type '{}' field '{}' should delegate to interface '{}' but does not",
                            type_name, interface_field_name, interface_name
                        );
                    };

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

    /// Validates that the macro invocation matches the schema. This will error if there are any
    /// types or fields in the macro invocation are not found in the schema registry.
    fn macro_invocation_matches_schema(registry: &Registry) {
        let type_fields = &TYPE_FIELD_DELEGATIONS;

        for ((type_name, field_name), _) in type_fields.iter() {
            let fields = match registry.types.get(*type_name) {
                Some(MetaType::Object { fields, .. } | MetaType::Interface { fields, .. }) => {
                    fields
                }
                Some(_) => panic!("Type '{}' is not an Object or Interface type", type_name),
                None => panic!("Type '{}' not found in schema registry", type_name),
            };
            fields.get(*field_name).unwrap_or_else(|| {
                panic!("Field '{}' not found in type '{}'", field_name, type_name)
            });
        }
    }

    /// Calls collect_pipeline on types, fields, and filters in the schema registry and
    /// stores the input and output in a snapshot for regression testing and auditing.
    /// If a filter does not result in a different set of pipelines from the unfiltered case,
    /// it is not included in the snapshot.
    fn registry_collect_pipelines_snapshot(registry: &Registry) {
        const PAGINATION_ARGS: &[&str] = &["first", "after", "last", "before"];

        let mut output = String::new();

        for (type_name, meta_type) in registry.types.iter() {
            let (MetaType::Object { fields, .. } | MetaType::Interface { fields, .. }) = meta_type
            else {
                continue;
            };

            for (field_name, meta_field) in fields.iter() {
                if should_ignore_in_snapshot(type_name, field_name) {
                    continue;
                }
                let filter_fields: Vec<String> = meta_field
                    .args
                    .iter()
                    .filter(|(name, _)| !PAGINATION_ARGS.contains(&name.as_str()))
                    .flat_map(|(param_name, meta_input_value)| {
                        let concrete_type = MetaTypeName::concrete_typename(&meta_input_value.ty);
                        match registry.types.get(concrete_type) {
                            Some(MetaType::InputObject { input_fields, .. }) => {
                                input_fields.keys().cloned().collect()
                            }
                            Some(MetaType::Scalar { .. }) => {
                                vec![param_name.clone()]
                            }
                            _ => vec![],
                        }
                    })
                    .collect();

                let mut unfiltered_pipelines = BTreeSet::new();
                super::collect_pipelines(
                    type_name,
                    Some(field_name),
                    BTreeSet::new(),
                    &mut unfiltered_pipelines,
                );
                let unfiltered_output_str =
                    formatted_output_str(type_name, field_name, &unfiltered_pipelines, None);
                output.push_str(&unfiltered_output_str);

                for filter_field in filter_fields.iter().map(Some) {
                    let filters = filter_field.iter().copied().map(String::from).collect();
                    let mut pipelines = BTreeSet::new();
                    super::collect_pipelines(type_name, Some(field_name), filters, &mut pipelines);
                    let output_str =
                        formatted_output_str(type_name, field_name, &pipelines, filter_field);
                    if unfiltered_pipelines != pipelines {
                        output.push_str(&output_str);
                    }
                }
            }
        }
        insta::assert_snapshot!(output);
    }

    fn formatted_output_str(
        type_name: &str,
        field_name: &str,
        pipelines: &BTreeSet<String>,
        filter_field: Option<&String>,
    ) -> String {
        let filter_suffix = filter_field.map_or(String::new(), |f| format!(" (filter: {f})"));
        let pipeline_list = pipelines
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ");

        let output_str =
            format!("{type_name}.{field_name}{filter_suffix}\n  => {{{pipeline_list}}}\n\n");
        output_str
    }

    /// If the type or fields is a generated type or field, or an introspection type, it is skipped in the snapshot.
    fn should_ignore_in_snapshot(type_name: &str, field_name: &str) -> bool {
        const GENERATED_TYPES: &[&str] = &["Connection", "Edge"];
        const GENERATED_FIELDS: &[&str] = &["node", "nodes", "edges", "cursor", "pageInfo"];
        const INTROSPECTION_TYPE: &str = "__";
        if type_name.starts_with(INTROSPECTION_TYPE)
            || GENERATED_TYPES
                .iter()
                .any(|suffix| type_name.ends_with(suffix))
                && GENERATED_FIELDS
                    .iter()
                    .any(|suffix| field_name.ends_with(suffix))
        {
            return true;
        }
        false
    }
}
