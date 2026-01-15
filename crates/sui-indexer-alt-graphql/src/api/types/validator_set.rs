// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Context as _;
use anyhow::anyhow;
use async_graphql::Context;
use async_graphql::Object;
use async_graphql::connection::Connection;
use move_core_types::account_address::AccountAddress;
use move_core_types::annotated_extractor as AE;
use move_core_types::annotated_value as A;
use move_core_types::annotated_visitor as AV;
use move_core_types::language_storage::StructTag;
use move_core_types::u256::U256;
use move_core_types::visitor_default;
use sui_types::SUI_SYSTEM_ADDRESS;
use sui_types::TypeTag;
use sui_types::base_types::SuiAddress as NativeSuiAddress;
use sui_types::sui_system_state::VALIDATOR_MODULE_NAME;
use sui_types::sui_system_state::VALIDATOR_STRUCT_NAME;

use crate::api::scalars::cursor::JsonCursor;
use crate::api::types::move_type::MoveType;
use crate::api::types::move_value::MoveValue;
use crate::api::types::validator::Validator;
use crate::error::RpcError;
use crate::pagination::Page;
use crate::pagination::PaginationConfig;
use crate::scope::Scope;

pub(crate) type CValidator = JsonCursor<usize>;

/// Representation of `0x3::validator_set::ValidatorSet`.
#[derive(Clone)]
pub(crate) struct ValidatorSet {
    pub(crate) contents: Arc<ValidatorSetContents>,
}

pub(crate) struct ValidatorSetContents {
    pub(crate) value: MoveValue,
    pub(crate) active_validators: Vec<ValidatorContents>,
}

pub(crate) struct ValidatorContents {
    pub(crate) bytes: Vec<u8>,
    pub(crate) reports: Vec<usize>,
    pub(crate) at_risk: u64,
}

/// Representation of `0x3::validator_set::ValidatorSet`.
#[Object]
impl ValidatorSet {
    /// The validators currently in the committee for this validator set.
    async fn active_validators(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<CValidator>,
        last: Option<u64>,
        before: Option<CValidator>,
    ) -> Option<Result<Connection<String, Validator>, RpcError>> {
        Some(
            async {
                let pagination: &PaginationConfig = ctx.data()?;
                let limits = pagination.limits("ValidatorSet", "activeValidators");
                let page = Page::from_params(limits, first, after, last, before)?;

                page.paginate_indices(self.contents.active_validators.len(), |idx| {
                    Ok(Validator {
                        contents: Arc::clone(&self.contents),
                        idx,
                    })
                })
            }
            .await,
        )
    }

    /// On-chain representation of the underlying `0x3::validator_set::ValidatorSet` value.
    async fn contents(&self) -> Option<&MoveValue> {
        Some(&self.contents.value)
    }
}

impl ValidatorSet {
    /// Construct a `ValidatorSet` by deserializing the relevant parts from a system state inner
    /// object.
    pub(crate) fn from_system_state(
        scope: Scope,
        bytes: &[u8],
        layout: &A::MoveTypeLayout,
    ) -> Result<Self, RpcError> {
        Ok(Self {
            contents: Arc::new(ValidatorSetContents::deserialize(scope, bytes, layout)?),
        })
    }
}

impl ValidatorSetContents {
    /// Extract the parts of a system state inner object that are relevant to its validator set:
    ///
    /// - `validators: ValidatorSet` raw bytes and layout,
    /// - `validators.active_validators: vector<Validator>` as individual raw bytes,
    /// - `validator_report_records: map<address, vector<address>>`.
    fn deserialize(
        scope: Scope,
        bytes: &[u8],
        layout: &A::MoveTypeLayout,
    ) -> Result<Self, RpcError> {
        type ReportRecords = BTreeMap<NativeSuiAddress, Vec<NativeSuiAddress>>;
        type AtRiskValidators = BTreeMap<NativeSuiAddress, u64>;

        #[derive(Default)]
        struct Contents<'b, 'l> {
            native: Option<(&'b [u8], &'l A::MoveTypeLayout)>,
            active_validators: Vec<(NativeSuiAddress, &'b [u8])>,
            report_records: Option<ReportRecords>,
            at_risk_validators: Option<AtRiskValidators>,
        }

        // Visitor to traverse a system state inner value looking for information related to the
        // validator set.
        enum Traversal<'c, 'b, 'l> {
            SystemState(&'c mut Contents<'b, 'l>),
            ValidatorSet(&'c mut Contents<'b, 'l>),
            ActiveValidators(&'c mut Contents<'b, 'l>),
        }

        // Simple visitor to extract the value of an `address`.
        struct AddressVisitor;

        #[derive(thiserror::Error, Debug)]
        enum Error {
            #[error(transparent)]
            Bcs(#[from] bcs::Error),

            #[error("Expected to find a 0x3::validator::Validator")]
            NotAValidator,

            #[error(transparent)]
            Visitor(#[from] AV::Error),
        }

        impl<'b, 'l> AV::Traversal<'b, 'l> for Traversal<'_, 'b, 'l> {
            type Error = Error;

            fn traverse_struct(
                &mut self,
                driver: &mut AV::StructDriver<'_, 'b, 'l>,
            ) -> Result<(), Error> {
                // When traversing a struct while under `ActiveValidators`, we are visiting a
                // particular validator.
                if let Traversal::ActiveValidators(c) = self {
                    if !driver.struct_layout().is_type(&ValidatorContents::tag()) {
                        return Err(Error::NotAValidator);
                    }

                    let lo = driver.position();
                    while driver.skip_field()?.is_some() {}
                    let hi = driver.position();

                    let path = vec![
                        AE::Element::Field("metadata"),
                        AE::Element::Field("sui_address"),
                        AE::Element::Type(&TypeTag::Address),
                    ];

                    let bytes = &driver.bytes()[lo..hi];
                    let layout = driver.struct_layout();
                    if let Some(address) =
                        AE::Extractor::deserialize_struct(bytes, layout, &mut AddressVisitor, path)?
                            .flatten()
                    {
                        c.active_validators.push((address, bytes));
                    }

                    return Ok(());
                }

                while let Some(field) = driver.peek_field() {
                    let name = field.name.as_str();
                    match self {
                        Traversal::SystemState(c) if name == "validators" => {
                            let lo = driver.position();
                            driver.next_field(&mut Traversal::ValidatorSet(c))?;
                            let hi = driver.position();

                            let bytes = &driver.bytes()[lo..hi];
                            c.native = Some((bytes, &field.layout));
                        }

                        Traversal::SystemState(c) if name == "validator_report_records" => {
                            let lo = driver.position();
                            driver.skip_field()?;
                            let hi = driver.position();

                            let bytes = &driver.bytes()[lo..hi];
                            let records: ReportRecords = bcs::from_bytes(bytes)?;
                            c.report_records = Some(records);
                        }

                        Traversal::ValidatorSet(c) if name == "active_validators" => {
                            let _ = driver.next_field(&mut Traversal::ActiveValidators(c))?;
                        }

                        Traversal::ValidatorSet(c) if name == "at_risk_validators" => {
                            let lo = driver.position();
                            driver.skip_field()?;
                            let hi = driver.position();

                            let bytes = &driver.bytes()[lo..hi];
                            let at_risk: AtRiskValidators = bcs::from_bytes(bytes)?;
                            c.at_risk_validators = Some(at_risk);
                        }

                        _ => {
                            let _ = driver.skip_field()?;
                        }
                    }
                }

                Ok(())
            }
        }

        impl AV::Visitor<'_, '_> for AddressVisitor {
            type Value = Option<NativeSuiAddress>;
            type Error = Error;

            visitor_default! { <'_, '_> u8, u16, u32, u64, u128, u256 = Ok(None) }
            visitor_default! { <'_, '_> bool, signer, vector, struct, variant = Ok(None) }

            fn visit_address(
                &mut self,
                _: &AV::ValueDriver<'_, '_, '_>,
                value: AccountAddress,
            ) -> Result<Self::Value, Error> {
                Ok(Some(value.into()))
            }
        }

        let mut contents = Contents::default();
        let mut traversal = Traversal::SystemState(&mut contents);
        A::MoveValue::visit_deserialize(bytes, layout, &mut traversal)
            .context("Failed to deserialize ValidatorSet")?;

        let Contents {
            native: Some((bytes, layout)),
            active_validators,
            report_records: Some(mut reports),
            at_risk_validators: Some(mut at_risk),
        } = contents
        else {
            return Err(anyhow!("ValidatorSet deserialization incomplete").into());
        };

        let address_to_index: BTreeMap<_, _> = active_validators
            .iter()
            .enumerate()
            .map(|(i, (addr, _))| (*addr, i))
            .collect();

        let active_validators = active_validators
            .into_iter()
            .map(|(addr, bytes)| {
                let reports = reports
                    .remove(&addr)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|v| address_to_index.get(&v).copied())
                    .collect();

                ValidatorContents {
                    bytes: bytes.to_owned(),
                    reports,
                    at_risk: at_risk.remove(&addr).unwrap_or_default(),
                }
            })
            .collect();

        Ok(Self {
            value: MoveValue {
                type_: MoveType::from_layout(layout.clone(), scope),
                native: bytes.to_owned(),
            },

            active_validators,
        })
    }

    pub(crate) fn scope(&self) -> &Scope {
        &self.value.type_.scope
    }
}

impl ValidatorContents {
    pub(crate) fn tag() -> StructTag {
        StructTag {
            address: SUI_SYSTEM_ADDRESS,
            module: VALIDATOR_MODULE_NAME.to_owned(),
            name: VALIDATOR_STRUCT_NAME.to_owned(),
            type_params: vec![],
        }
    }
}
