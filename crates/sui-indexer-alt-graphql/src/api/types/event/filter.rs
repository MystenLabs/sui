// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::InputObject;
use sui_rpc::proto::sui::rpc::v2::EmitModuleFilter;
use sui_rpc::proto::sui::rpc::v2::EventFilter as GrpcEventFilter;
use sui_rpc::proto::sui::rpc::v2::EventLiteral;
use sui_rpc::proto::sui::rpc::v2::EventTerm;
use sui_rpc::proto::sui::rpc::v2::EventTypeFilter;
use sui_rpc::proto::sui::rpc::v2::SenderFilter;
use sui_rpc::proto::sui::rpc::v2::event_literal::Predicate;
use sui_types::event::Event as NativeEvent;

use crate::api::scalars::module_filter::ModuleFilter;
use crate::api::scalars::sui_address::SuiAddress;
use crate::api::scalars::type_filter::TypeFilter;
use crate::api::scalars::uint53::UInt53;
use crate::api::types::lookups::CheckpointBounds;
use crate::error::RpcError;
use crate::error::feature_unavailable;

#[derive(InputObject, Debug, Default, Clone)]
pub(crate) struct EventFilter {
    /// Limit to events that occured strictly after the given checkpoint.
    pub after_checkpoint: Option<UInt53>,

    /// Limit to events in the given checkpoint.
    pub at_checkpoint: Option<UInt53>,

    /// Limit to event that occured strictly before the given checkpoint.
    pub before_checkpoint: Option<UInt53>,

    /// Filter on events by transaction sender address.
    pub sender: Option<SuiAddress>,

    /// Events emitted by a particular module. An event is emitted by a particular module if some function in the module is called by a PTB and emits an event.
    ///
    /// Modules can be filtered by their package, or package::module. We currently do not support filtering by emitting module and event type at the same time so if both are provided in one filter, the query will error.
    pub module: Option<ModuleFilter>,

    /// This field is used to specify the type of event emitted.
    ///
    /// Events can be filtered by their type's package, package::module, or their fully qualified type name.
    ///
    /// Generic types can be queried by either the generic type name, e.g. `0x2::coin::Coin`, or by the full type name, such as `0x2::coin::Coin<0x2::sui::SUI>`.
    pub type_: Option<TypeFilter>,
}

impl EventFilter {
    /// Check if the Event matches sender, module, or type filters in EventFilter if they are
    /// provided. Used by the `staging`-gated subscription backfill, which filters events
    /// client-side rather than through the ledger gRPC list API — unused (and so
    /// `#[allow(dead_code)]`'d) otherwise.
    #[cfg_attr(not(feature = "staging"), allow(dead_code))]
    pub(crate) fn matches(&self, event: &NativeEvent) -> bool {
        if let Some(sender) = &self.sender
            && sender != &SuiAddress::from(event.sender)
        {
            return false;
        }

        if let Some(module) = &self.module {
            if module.package() != SuiAddress::from(event.package_id) {
                return false;
            }

            if let Some(module) = module.module()
                && module != event.transaction_module.as_str()
            {
                return false;
            }
        }

        if let Some(type_) = &self.type_ {
            if type_.package() != SuiAddress::from(event.type_.address) {
                return false;
            }

            if let Some(module) = type_.module()
                && module != event.type_.module.as_str()
            {
                return false;
            }

            if let Some(type_name) = type_.type_name()
                && type_name != event.type_.name.as_str()
            {
                return false;
            }

            if let Some(type_params) = type_.type_params()
                && type_params != event.type_.type_params.as_slice()
            {
                return false;
            }
        }

        true
    }

    /// Translate the filter into the gRPC `EventFilter` DNF shape: a single term whose literals are
    /// the conjunction of the set fields. Checkpoint bounds are not part of the filter — they map
    /// to the request's checkpoint range. Returns `None` when no predicate field is set (an absent
    /// filter matches everything in range).
    ///
    /// Errors when both `module` and `type_` are set — the bitmap engine supports the conjunction,
    /// but lifting the restriction is deferred to a future change.
    pub(crate) fn to_grpc_filter(&self) -> Result<Option<GrpcEventFilter>, RpcError> {
        if self.module.is_some() && self.type_.is_some() {
            return Err(feature_unavailable(
                "Filtering by both emitting module and event type is not supported",
            ));
        }

        let mut literals = Vec::new();

        if let Some(sender) = &self.sender {
            literals.push(include_literal(sender_predicate(sender)));
        }
        if let Some(module) = &self.module {
            literals.push(include_literal(emit_module_predicate(module)));
        }
        if let Some(type_) = &self.type_ {
            literals.push(include_literal(event_type_predicate(type_)));
        }

        if literals.is_empty() {
            return Ok(None);
        }

        let filter = GrpcEventFilter::default()
            .with_terms(vec![EventTerm::default().with_literals(literals)]);

        Ok(Some(filter))
    }
}

impl CheckpointBounds for EventFilter {
    fn after_checkpoint(&self) -> Option<UInt53> {
        self.after_checkpoint
    }

    fn at_checkpoint(&self) -> Option<UInt53> {
        self.at_checkpoint
    }

    fn before_checkpoint(&self) -> Option<UInt53> {
        self.before_checkpoint
    }
}

fn include_literal(predicate: Predicate) -> EventLiteral {
    let mut literal = EventLiteral::default();
    literal.predicate = Some(predicate);
    literal
}

fn sender_predicate(address: &SuiAddress) -> Predicate {
    let mut f = SenderFilter::default();
    f.address = Some(address.to_string());
    Predicate::Sender(f)
}

fn emit_module_predicate(module: &ModuleFilter) -> Predicate {
    let mut f = EmitModuleFilter::default();
    f.module = Some(module.to_string());
    Predicate::EmitModule(f)
}

fn event_type_predicate(type_: &TypeFilter) -> Predicate {
    let mut f = EventTypeFilter::default();
    f.event_type = Some(type_.to_string());
    Predicate::EventType(f)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn addr(s: &str) -> SuiAddress {
        s.parse().expect("valid address")
    }

    /// Extract the predicates from a single-term filter, asserting no literal is
    /// negated (the parity shape).
    fn term_includes(filter: &GrpcEventFilter) -> Vec<&Predicate> {
        assert_eq!(filter.terms.len(), 1, "parity filter is a single term");
        filter.terms[0]
            .literals
            .iter()
            .map(|literal| {
                assert!(!literal.negated, "expected non-negated literal");
                literal.predicate.as_ref().expect("predicate set")
            })
            .collect()
    }

    #[test]
    fn unfiltered_yields_no_grpc_filter() {
        let filter = EventFilter::default();
        assert!(filter.to_grpc_filter().expect("serviceable").is_none());
    }

    #[test]
    fn checkpoint_bounds_alone_are_not_a_predicate() {
        let filter = EventFilter {
            after_checkpoint: Some(UInt53::from(10)),
            before_checkpoint: Some(UInt53::from(20)),
            ..Default::default()
        };
        // Checkpoint bounds map to the request's checkpoint range, not the filter.
        assert!(filter.to_grpc_filter().expect("serviceable").is_none());
    }

    #[test]
    fn sender_maps_to_sender_include() {
        let sender = addr("0x2");
        let filter = EventFilter {
            sender: Some(sender),
            ..Default::default()
        };

        let proto = filter
            .to_grpc_filter()
            .expect("serviceable")
            .expect("filter present");
        let predicates = term_includes(&proto);
        assert_eq!(predicates.len(), 1);
        match predicates[0] {
            Predicate::Sender(f) => {
                assert_eq!(f.address.as_deref(), Some(sender.to_string().as_str()));
            }
            other => panic!("expected Sender, got {other:?}"),
        }
    }

    #[test]
    fn module_maps_to_emit_module_include_at_each_specificity() {
        for input in ["0x42", "0x42::m"] {
            let module = ModuleFilter::from_str(input).expect("valid module filter");
            let expected = module.to_string();
            let filter = EventFilter {
                module: Some(module),
                ..Default::default()
            };

            let proto = filter
                .to_grpc_filter()
                .expect("serviceable")
                .expect("filter present");
            let predicates = term_includes(&proto);
            assert_eq!(predicates.len(), 1);
            match predicates[0] {
                Predicate::EmitModule(f) => {
                    assert_eq!(f.module.as_deref(), Some(expected.as_str()));
                }
                other => panic!("expected EmitModule, got {other:?}"),
            }
        }
    }

    #[test]
    fn type_maps_to_event_type_include_at_each_specificity() {
        for input in ["0x42", "0x42::m", "0x42::m::T", "0x42::m::T<0x2::sui::SUI>"] {
            let type_ = TypeFilter::from_str(input).expect("valid type filter");
            let expected = type_.to_string();
            let filter = EventFilter {
                type_: Some(type_),
                ..Default::default()
            };

            let proto = filter
                .to_grpc_filter()
                .expect("serviceable")
                .expect("filter present");
            let predicates = term_includes(&proto);
            assert_eq!(predicates.len(), 1);
            match predicates[0] {
                Predicate::EventType(f) => {
                    assert_eq!(f.event_type.as_deref(), Some(expected.as_str()));
                }
                other => panic!("expected EventType, got {other:?}"),
            }
        }
    }

    #[test]
    fn sender_combines_with_module_or_type_in_one_term() {
        let sender = addr("0x2");

        let with_module = EventFilter {
            sender: Some(sender),
            module: Some(ModuleFilter::from_str("0x42::m").expect("valid module filter")),
            ..Default::default()
        }
        .to_grpc_filter()
        .expect("serviceable")
        .expect("filter present");
        let preds = term_includes(&with_module);
        assert_eq!(preds.len(), 2);
        assert!(matches!(preds[0], Predicate::Sender(_)));
        assert!(matches!(preds[1], Predicate::EmitModule(_)));

        let with_type = EventFilter {
            sender: Some(sender),
            type_: Some(TypeFilter::from_str("0x42::m::T").expect("valid type filter")),
            ..Default::default()
        }
        .to_grpc_filter()
        .expect("serviceable")
        .expect("filter present");
        let preds = term_includes(&with_type);
        assert_eq!(preds.len(), 2);
        assert!(matches!(preds[0], Predicate::Sender(_)));
        assert!(matches!(preds[1], Predicate::EventType(_)));
    }

    #[test]
    fn module_and_type_together_are_rejected() {
        let filter = EventFilter {
            module: Some(ModuleFilter::from_str("0x42::m").expect("valid module filter")),
            type_: Some(TypeFilter::from_str("0x42::m::T").expect("valid type filter")),
            ..Default::default()
        };
        assert!(filter.to_grpc_filter().is_err());
    }
}
