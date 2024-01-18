// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use super::{
    cursor::Page,
    move_object::MoveObject,
    object::{self, Object, ObjectFilter},
    string_input::impl_string_input,
    sui_address::SuiAddress,
};
use crate::{data::Db, error::Error};
use async_graphql::{connection::Connection, *};
use move_core_types::{ident_str, identifier::IdentStr, language_storage::StructTag};
use serde::{Deserialize, Serialize};
use sui_json_rpc::name_service::{Domain as NativeDomain, NameRecord, NameServiceConfig};
use sui_types::{base_types::SuiAddress as NativeSuiAddress, dynamic_field::Field, id::UID};

const MOD_REGISTRATION: &IdentStr = ident_str!("suins_registration");
const TYP_REGISTRATION: &IdentStr = ident_str!("SuinsRegistration");

/// Wrap SuiNS Domain type to expose as a string scalar in GraphQL.
#[derive(Debug)]
pub(crate) struct Domain(NativeDomain);

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct NativeSuinsRegistration {
    pub id: UID,
    pub domain: NativeDomain,
    pub domain_name: String,
    pub expiration_timestamp_ms: u64,
    pub image_url: String,
}

#[derive(Clone)]
pub(crate) struct SuinsRegistration {
    /// Representation of this SuinsRegistration as a generic Move object.
    pub super_: MoveObject,

    /// The deserialized representation of the Move object's contents.
    pub native: NativeSuinsRegistration,
}

pub(crate) enum SuinsRegistrationDowncastError {
    NotASuinsRegistration,
    Bcs(bcs::Error),
}

#[Object]
impl SuinsRegistration {
    /// Domain name of the SuinsRegistration object
    async fn domain(&self) -> &str {
        &self.native.domain_name
    }

    /// Convert the SuinsRegistration object into a Move object
    async fn as_move_object(&self) -> &MoveObject {
        &self.super_
    }
}

impl SuinsRegistration {
    /// Lookup the SuiNS NameRecord for the given `domain` name. `config` specifies where to find
    /// the domain name registry, and its type.
    pub(crate) async fn resolve_to_record(
        db: &Db,
        config: &NameServiceConfig,
        domain: &Domain,
    ) -> Result<Option<NameRecord>, Error> {
        let record_id = config.record_field_id(&domain.0);

        let Some(object) = MoveObject::query(db, record_id.into(), None).await? else {
            return Ok(None);
        };

        let field: Field<NativeDomain, NameRecord> = object
            .native
            .to_rust()
            .ok_or_else(|| Error::Internal("Malformed Suins NameRecord".to_string()))?;

        Ok(Some(field.value))
    }

    /// Lookup the SuiNS Domain for the given `address`. `config` specifies where to find the domain
    /// name registry, and its type.
    pub(crate) async fn reverse_resolve_to_name(
        db: &Db,
        config: &NameServiceConfig,
        address: SuiAddress,
    ) -> Result<Option<NativeDomain>, Error> {
        let reverse_record_id = config.reverse_record_field_id(address.as_slice());

        let Some(object) = MoveObject::query(db, reverse_record_id.into(), None).await? else {
            return Ok(None);
        };

        let field: Field<NativeSuiAddress, NativeDomain> = object
            .native
            .to_rust()
            .ok_or_else(|| Error::Internal("Malformed Suins Domain".to_string()))?;

        Ok(Some(field.value))
    }

    /// Query the database for a `page` of SuiNS registrations. The page uses the same cursor type
    /// as is used for `Object`, and is further filtered to a particular `owner`. `config` specifies
    /// where to find the domain name registry and its type.
    pub(crate) async fn paginate(
        db: &Db,
        config: &NameServiceConfig,
        page: Page<object::Cursor>,
        owner: SuiAddress,
    ) -> Result<Connection<String, SuinsRegistration>, Error> {
        let type_ = SuinsRegistration::type_(config.package_address.into());

        let filter = ObjectFilter {
            type_: Some(type_.clone().into()),
            owner: Some(owner),
            ..Default::default()
        };

        Object::paginate_subtype(db, page, filter, |object| {
            let address = object.address;
            let move_object = MoveObject::try_from(&object).map_err(|_| {
                Error::Internal(format!(
                    "Expected {address} to be a SuinsRegistration, but it's not a Move Object.",
                ))
            })?;

            SuinsRegistration::try_from(&move_object, &type_).map_err(|_| {
                Error::Internal(format!(
                    "Expected {address} to be a SuinsRegistration, but it is not."
                ))
            })
        })
        .await
    }

    /// Return the type representing a `SuinsRegistration` on chain. This can change from chain to
    /// chain (mainnet, testnet, devnet etc).
    pub(crate) fn type_(package: SuiAddress) -> StructTag {
        StructTag {
            address: package.into(),
            module: MOD_REGISTRATION.to_owned(),
            name: TYP_REGISTRATION.to_owned(),
            type_params: vec![],
        }
    }

    // Because the type of the SuinsRegistration object is not constant,
    // we need to take it in as a param.
    pub(crate) fn try_from(
        move_object: &MoveObject,
        tag: &StructTag,
    ) -> Result<Self, SuinsRegistrationDowncastError> {
        if !move_object.native.is_type(tag) {
            return Err(SuinsRegistrationDowncastError::NotASuinsRegistration);
        }

        Ok(Self {
            super_: move_object.clone(),
            native: bcs::from_bytes(move_object.native.contents())
                .map_err(SuinsRegistrationDowncastError::Bcs)?,
        })
    }
}

impl_string_input!(Domain);

impl FromStr for Domain {
    type Err = <NativeDomain as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Domain(NativeDomain::from_str(s)?))
    }
}
