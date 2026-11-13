// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, OnceLock};

use futures::FutureExt;
use move_core_types::annotated_value::MoveTypeLayout;
use move_core_types::language_storage::{StructTag, TypeTag};
use mysten_common::ZipDebugEqIteratorExt;
use prost_types::Struct;
use sui_package_resolver::{Limits as ResolverLimits, PackageStoreWithLruCache, SyncResolver};
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_rpc::{
    field::FieldMaskTree,
    merge::Merge,
    proto::sui::rpc::v2::{
        Bcs, Display, Event, ExecutedTransaction, Object, TransactionEffects, TransactionEvents,
    },
};
use sui_types::full_checkpoint_content::ObjectSet;
use sui_types::storage::{BackingPackageStore, OverlayBackingPackageStore, RpcStateReader};

use crate::{RpcService, reader::DisplayStore};

type CachedLayoutResolver<'a> =
    SyncResolver<PackageStoreWithLruCache<Box<dyn BackingPackageStore + Send + Sync + 'a>>>;

fn resolver_limits() -> ResolverLimits {
    static LIMITS: OnceLock<ResolverLimits> = OnceLock::new();

    LIMITS
        .get_or_init(|| ResolverLimits {
            max_type_argument_depth: max_across_protocol(
                ProtocolConfig::max_type_argument_depth_as_option,
            )
            .unwrap_or(u32::MAX) as usize,
            max_type_argument_width: max_across_protocol(
                ProtocolConfig::max_generic_instantiation_length_as_option,
            )
            .unwrap_or(u32::MAX as u64) as usize,
            max_type_nodes: max_across_protocol(ProtocolConfig::max_type_nodes_as_option)
                .unwrap_or(u32::MAX as u64) as usize,
            max_move_value_depth: max_across_protocol(
                ProtocolConfig::max_move_value_depth_as_option,
            )
            .unwrap_or(u32::MAX as u64) as usize,
            max_layout_nodes: max_across_protocol(
                ProtocolConfig::max_type_to_layout_nodes_as_option,
            )
            .unwrap_or(512) as usize,
        })
        .clone()
}

fn max_across_protocol<T: Ord>(f: impl Fn(&ProtocolConfig) -> Option<T>) -> Option<T> {
    let mut max = None;
    let mut version = ProtocolVersion::MIN;
    while version <= ProtocolVersion::MAX {
        max = max.max(f(&ProtocolConfig::get_for_version(version, Chain::Unknown)));
        max = max.max(f(&ProtocolConfig::get_for_version(version, Chain::Mainnet)));
        max = max.max(f(&ProtocolConfig::get_for_version(version, Chain::Testnet)));
        version = version + 1;
    }
    max
}

/// Reuses parsed packages while rendering a response against a fixed object overlay.
pub(crate) struct RenderContext<'a> {
    reader: &'a dyn RpcStateReader,
    overlay: Option<&'a ObjectSet>,
    layout_resolver: OnceLock<CachedLayoutResolver<'a>>,
}

impl<'a> RenderContext<'a> {
    pub(crate) fn empty(reader: &'a Arc<dyn RpcStateReader>) -> Self {
        Self::new(reader, None)
    }

    pub(crate) fn new(reader: &'a Arc<dyn RpcStateReader>, overlay: Option<&'a ObjectSet>) -> Self {
        Self {
            reader: reader.as_ref(),
            // An empty overlay can use the backing store directly.
            overlay: overlay.filter(|objects| !objects.is_empty()),
            layout_resolver: OnceLock::new(),
        }
    }

    pub(crate) fn type_layout(
        &self,
        type_tag: TypeTag,
    ) -> sui_package_resolver::Result<MoveTypeLayout> {
        self.layout_resolver().type_layout(type_tag)
    }

    fn layout_resolver(&self) -> &CachedLayoutResolver<'a> {
        self.layout_resolver.get_or_init(|| {
            let mut store = self.reader.layout_package_store();
            if let Some(overlay) = self.overlay {
                store = Box::new(OverlayBackingPackageStore::new(overlay, store));
            }
            SyncResolver::new_with_limits(PackageStoreWithLruCache::new(store), resolver_limits())
        })
    }
}

impl RpcService {
    pub(crate) fn render_object_to_proto(
        &self,
        object: &sui_types::object::Object,
        read_mask: &FieldMaskTree,
        context: &RenderContext,
    ) -> Object {
        let mut message = Object::default();

        if read_mask.contains(Object::JSON_FIELD) {
            let move_object = object.data.try_as_move();
            message.json = move_object.and_then(|m| {
                self.render_json(&m.type_().clone().into(), m.contents(), context)
                    .map(Box::new)
            });
        }

        if read_mask.contains(Object::DISPLAY_FIELD) {
            message.display = self.render_object_display(object, context).map(Box::new);
        }

        message.merge(object, read_mask);

        message
    }

    /// Render a Move value as JSON using the response's package cache and overlay.
    fn render_json(
        &self,
        struct_tag: &StructTag,
        contents: &[u8],
        context: &RenderContext,
    ) -> Option<prost_types::Value> {
        let mut budget = self.config.max_json_move_value_size();
        self.render_json_with_budget(struct_tag, contents, context, &mut budget)
    }

    /// Render a Move value as JSON, drawing the size budget from a caller-owned
    /// counter. Endpoints that render many Move values in a single response
    /// (e.g. every event in a checkpoint) share one budget across all renders
    /// to bound the aggregate response memory rather than multiplying the
    /// per-render limit by the number of items. The budget is updated in place
    /// to reflect bytes consumed; once it reaches zero subsequent calls fail
    /// fast and return `None`.
    fn render_json_with_budget(
        &self,
        struct_tag: &StructTag,
        contents: &[u8],
        context: &RenderContext,
        size_budget: &mut usize,
    ) -> Option<prost_types::Value> {
        let layout = context
            .type_layout(TypeTag::Struct(Box::new(struct_tag.clone())))
            .ok()?;

        sui_types::object::rpc_visitor::proto::ProtoVisitor::deserialize_value_with_budget(
            contents,
            &layout,
            size_budget,
        )
        .map_err(|e| tracing::debug!("unable to convert move value to JSON: {e}"))
        .ok()
    }

    fn render_object_display(
        &self,
        object: &sui_types::object::Object,
        context: &RenderContext,
    ) -> Option<Display> {
        let move_object = object.data.try_as_move()?;
        let object_type = &move_object.type_().clone().into();
        let contents = move_object.contents();

        let limits = sui_display::v2::Limits {
            max_depth: self.config.display().max_field_depth(),
            max_nodes: self.config.display().max_format_nodes(),
            max_loads: self.config.display().max_object_loads(),
        };
        let display_object = self.reader.get_display_object_v2_by_type(object_type)?;
        let display_template =
            sui_display::v2::Display::parse(limits, display_object.fields()).ok()?;

        let resolver = context.layout_resolver();
        let layout = resolver.datatype_layout(object_type).ok()?.into_layout();

        let root = sui_display::v2::OwnedSlice::new(layout, contents.to_owned());
        let interpreter =
            sui_display::v2::Interpreter::new(root, DisplayStore::new(&self.reader, resolver));

        let mut display = Display::default();

        // The display api requires that the `sui_display::v2::Store` is async. We know that the
        // Store implementation we are passing in here is fully synchronous, doing db access
        // in-place and as such should never return Poll::Pending.
        match display_template
            .display::<prost_types::Value>(
                self.config.display().max_move_value_depth(),
                self.config.display().max_output_size(),
                &interpreter,
            )
            .now_or_never()
            .unwrap()
        {
            Ok(rendered) => {
                let mut output = Struct::default();
                let mut errors = Struct::default();

                for (field, result) in rendered {
                    match result {
                        Ok(value) => {
                            output.fields.insert(field, value);
                        }
                        Err(e) => {
                            errors.fields.insert(field, e.to_string().into());
                        }
                    }
                }

                if !output.fields.is_empty() {
                    display.set_output(output.fields);
                }

                if !errors.fields.is_empty() {
                    display.set_errors(errors.fields);
                }
            }
            Err(e) => {
                display.set_errors(e.to_string());
            }
        }

        Some(display)
    }

    pub(crate) fn render_events_to_proto(
        &self,
        events: &sui_types::effects::TransactionEvents,
        mask: &FieldMaskTree,
        context: &RenderContext,
    ) -> TransactionEvents {
        let mut message = TransactionEvents::default();

        if mask.contains(TransactionEvents::BCS_FIELD) {
            let mut bcs = Bcs::serialize(&events).unwrap();
            bcs.name = Some("TransactionEvents".to_owned());
            message.bcs = Some(bcs);
        }

        if mask.contains(TransactionEvents::DIGEST_FIELD) {
            message.digest = Some(events.digest().to_string());
        }

        if let Some(event_mask) = mask.subtree(TransactionEvents::EVENTS_FIELD) {
            message.events = events
                .data
                .iter()
                .map(|event| self.render_event_to_proto(event, &event_mask, context))
                .collect();
        }

        message
    }

    pub(crate) fn render_event_to_proto(
        &self,
        event: &sui_types::event::Event,
        mask: &FieldMaskTree,
        context: &RenderContext,
    ) -> Event {
        let mut message = Event::default();

        if mask.contains(Event::PACKAGE_ID_FIELD) {
            message.set_package_id(event.package_id.to_canonical_string(true));
        }

        if mask.contains(Event::MODULE_FIELD) {
            message.set_module(event.transaction_module.to_string());
        }

        if mask.contains(Event::SENDER_FIELD) {
            message.sender = Some(event.sender.to_string());
        }

        if mask.contains(Event::EVENT_TYPE_FIELD) {
            message.event_type = Some(event.type_.to_canonical_string(true));
        }

        if mask.contains(Event::CONTENTS_FIELD) {
            let mut bcs = Bcs::from(event.contents.clone());
            bcs.name = Some(event.type_.to_canonical_string(true));
            message.contents = Some(bcs);
        }

        if mask.contains(Event::JSON_FIELD) {
            message.json = self
                .render_json(&event.type_, &event.contents, context)
                .map(Box::new);
        }

        message
    }

    // Renders clever error information in-place
    pub fn render_clever_error(&self, effects: &mut TransactionEffects) {
        use sui_rpc::proto::sui::rpc::v2::CleverError;
        use sui_rpc::proto::sui::rpc::v2::MoveAbort;
        use sui_rpc::proto::sui::rpc::v2::clever_error;
        use sui_rpc::proto::sui::rpc::v2::execution_error::ErrorDetails;

        let Some(move_abort) = effects
            .status
            .as_mut()
            .and_then(|status| status.error.as_mut())
            .and_then(|error| match &mut error.error_details {
                Some(ErrorDetails::Abort(move_abort)) => Some(move_abort),
                _ => None,
            })
        else {
            return;
        };

        fn render(service: &RpcService, move_abort: &MoveAbort) -> Option<CleverError> {
            let location = move_abort.location.as_ref()?;
            let abort_code = move_abort.abort_code();
            let package_id = location.package().parse::<sui_sdk_types::Address>().ok()?;
            let module = location.module();

            let package = {
                let object = service.reader.inner().get_object(&package_id.into())?;
                sui_package_resolver::Package::read_from_object(&object).ok()?
            };

            let clever_error = package.resolve_clever_error(module, abort_code)?;

            let mut clever_error_message = CleverError::default();

            match clever_error.error_info {
                sui_package_resolver::ErrorConstants::None => {}
                sui_package_resolver::ErrorConstants::Rendered {
                    identifier,
                    constant,
                } => {
                    clever_error_message.constant_name = Some(identifier);
                    clever_error_message.value = Some(clever_error::Value::Rendered(constant));
                }
                sui_package_resolver::ErrorConstants::Raw { identifier, bytes } => {
                    clever_error_message.constant_name = Some(identifier);
                    clever_error_message.value = Some(clever_error::Value::Raw(bytes.into()));
                }
            }

            clever_error_message.error_code = clever_error.error_code.map(Into::into);
            clever_error_message.line_number = Some(clever_error.source_line_number.into());

            Some(clever_error_message)
        }

        move_abort.clever_error = render(self, move_abort);
    }

    pub fn render_effects_to_proto(
        &self,
        effects: &sui_types::effects::TransactionEffects,
        unchanged_loaded_runtime_objects: &[sui_types::storage::ObjectKey],
        objects: &ObjectSet,
        mask: &FieldMaskTree,
    ) -> TransactionEffects {
        // TODO consider inlining this function here to avoid needing to do the extra parsing below
        let mut message = TransactionEffects::merge_from(effects, mask);

        if mask.contains(TransactionEffects::UNCHANGED_LOADED_RUNTIME_OBJECTS_FIELD) {
            message.unchanged_loaded_runtime_objects = unchanged_loaded_runtime_objects
                .iter()
                .map(Into::into)
                .collect();
        }

        self.annotate_effects(&mut message, objects, mask);

        message
    }

    /// Fill the effects fields that `TransactionEffects::merge_from` cannot: the
    /// object types of changed and unchanged consensus objects, looked up in
    /// `objects`, and the clever error of a Move abort.
    pub fn annotate_effects(
        &self,
        effects: &mut TransactionEffects,
        objects: &ObjectSet,
        mask: &FieldMaskTree,
    ) {
        if mask.contains(TransactionEffects::CHANGED_OBJECTS_FIELD) {
            for changed_object in effects.changed_objects.iter_mut() {
                let Ok(object_id) = changed_object
                    .object_id()
                    .parse::<sui_types::base_types::ObjectID>()
                else {
                    continue;
                };

                if let Some(object) = objects.get(&sui_types::storage::ObjectKey(
                    object_id,
                    changed_object
                        .input_version_opt()
                        .unwrap_or_else(|| changed_object.output_version())
                        .into(),
                )) {
                    changed_object.set_object_type(object_type_to_string(object.into()));
                }
            }
        }

        if mask.contains(TransactionEffects::UNCHANGED_CONSENSUS_OBJECTS_FIELD) {
            for unchanged_consensus_object in effects.unchanged_consensus_objects.iter_mut() {
                let Ok(object_id) = unchanged_consensus_object
                    .object_id()
                    .parse::<sui_types::base_types::ObjectID>()
                else {
                    continue;
                };

                if let Some(object) = objects.get(&sui_types::storage::ObjectKey(
                    object_id,
                    unchanged_consensus_object.version().into(),
                )) {
                    unchanged_consensus_object
                        .set_object_type(object_type_to_string(object.into()));
                }
            }
        }

        // Try to render clever error info
        self.render_clever_error(effects);
    }

    /// Fill the parts of an `ExecutedTransaction` merged from checkpoint data
    /// that `merge_from` cannot: effects object types and clever errors (see
    /// [`Self::annotate_effects`]) and event JSON, which needs the package
    /// resolver. Event JSON draws from `json_budget`, which the caller shares
    /// across every event in one response.
    pub(crate) fn annotate_checkpoint_transaction(
        &self,
        message: &mut ExecutedTransaction,
        transaction: &sui_types::full_checkpoint_content::ExecutedTransaction,
        objects: &ObjectSet,
        mask: &FieldMaskTree,
        json_budget: &mut usize,
        context: &RenderContext,
    ) {
        if let Some(effects_mask) = mask.subtree(ExecutedTransaction::EFFECTS_FIELD)
            && let Some(effects) = message.effects.as_mut()
        {
            self.annotate_effects(effects, objects, &effects_mask);
        }

        if let Some(events_mask) = mask.subtree(ExecutedTransaction::EVENTS_FIELD)
            && let Some(event_mask) = events_mask.subtree(TransactionEvents::EVENTS_FIELD)
            && event_mask.contains(Event::JSON_FIELD)
            && let Some(rendered) = message.events.as_mut()
            && let Some(events) = &transaction.events
        {
            for (event_message, event) in rendered.events.iter_mut().zip_debug_eq(&events.data) {
                event_message.json = self
                    .render_json_with_budget(&event.type_, &event.contents, context, json_budget)
                    .map(Box::new);
            }
        }
    }
}

fn object_type_to_string(object_type: sui_types::base_types::ObjectType) -> String {
    match object_type {
        sui_types::base_types::ObjectType::Package => "package".to_owned(),
        sui_types::base_types::ObjectType::Struct(move_object_type) => {
            move_object_type.to_canonical_string(true)
        }
    }
}
