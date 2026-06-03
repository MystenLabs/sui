// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `list_owned_objects` + `list_objects_by_type` handlers.
//!
//! `list_owned_objects` paginates over the `object_by_owner` CF
//! at a snapshot, narrowed by the owner kind (address / object /
//! shared / immutable) and, optionally, a `TypeFilter`. The
//! `!` exclusion prefix is honoured: a request for
//! `!0x2::coin::Coin<SUI>` returns all owned objects *except*
//! that type. Internally that's the alt-consistent-store's
//! `paginate_exclude` shape — for v1 here we implement the
//! simpler "filter on the post-decode key" variant and leave
//! a TODO to push that into the byte iterator if it becomes a
//! hot path.
//!
//! `list_objects_by_type` is structurally the same but paginates
//! over the `object_by_type` CF — every object of a given Move
//! type, regardless of owner.

use sui_consistent_store::Schema as _;
use sui_consistent_store::SchemaAtSnapshot as _;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as grpc;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::owner::OwnerKind as ProtoOwnerKind;
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::schema::object_by_owner;
use sui_rpc_store::schema::type_filter::TypeFilter;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::parse_sui_address;
use sui_types::parse_sui_module_id;
use sui_types::parse_sui_struct_tag;

use crate::consistent_service::State;
use crate::consistent_service::pagination::End;
use crate::consistent_service::pagination::Page;
use crate::consistent_service::state::Error as StateError;

#[derive(Debug, thiserror::Error)]
pub(super) enum Error {
    #[error("missing owner")]
    MissingOwner,
    #[error("missing object_type")]
    MissingType,
    #[error("invalid owner kind")]
    InvalidOwnerKind,
    #[error("invalid owner: {0:?}")]
    InvalidOwner(String),
    #[error("invalid object_type: {0:?}")]
    InvalidType(String),
    #[error(transparent)]
    Db(#[from] sui_consistent_store::error::Error),
    #[error("failed to open schema: {0}")]
    Open(#[from] sui_consistent_store::error::OpenError),
    #[error(transparent)]
    State(#[from] StateError),
}

impl From<Error> for tonic::Status {
    fn from(e: Error) -> Self {
        match e {
            Error::MissingOwner
            | Error::MissingType
            | Error::InvalidOwnerKind
            | Error::InvalidOwner(_)
            | Error::InvalidType(_) => tonic::Status::invalid_argument(e.to_string()),
            Error::Db(_) | Error::Open(_) => tonic::Status::internal(e.to_string()),
            Error::State(s) => tonic::Status::from(s),
        }
    }
}

pub(super) fn list_owned_objects(
    state: &State,
    checkpoint: u64,
    request: grpc::ListOwnedObjectsRequest,
) -> Result<grpc::ListObjectsResponse, Error> {
    let owner = request.owner.as_ref().ok_or(Error::MissingOwner)?;
    let kind = parse_owner(owner)?;
    let (filter, negated) = parse_filter(request.object_type.as_deref())?;

    let snap = state.snapshot(checkpoint)?;
    let schema = RpcStoreSchema::open(&state.db)?.at(&snap);
    let page = Page::from_request(
        &state.pagination,
        request.after_token(),
        request.before_token(),
        request.page_size(),
        End::from_proto(request.end.unwrap_or_default()),
    );

    // We model the "single owner address" listings via the
    // existing prefix encoders. Shared / Immutable scans iterate
    // the entire owner-kind range using a tiny ad-hoc encoder so
    // we share the same pagination plumbing.
    let resp = match kind {
        OwnerSelector::Address(addr) => {
            let prefix = object_by_owner::AddressOwnerPrefix(addr);
            page.paginate_filtered(&schema.object_by_owner, &prefix, |_, key, _| {
                matches_type_filter(filter.as_ref(), negated, &key.type_)
            })?
        }
        OwnerSelector::Object(addr) => {
            let prefix = object_by_owner::ObjectOwnerPrefix(addr);
            page.paginate_filtered(&schema.object_by_owner, &prefix, |_, key, _| {
                matches_type_filter(filter.as_ref(), negated, &key.type_)
            })?
        }
        OwnerSelector::Shared => {
            let prefix = KindPrefix(2);
            page.paginate_filtered(&schema.object_by_owner, &prefix, |_, key, _| {
                matches_type_filter(filter.as_ref(), negated, &key.type_)
            })?
        }
        OwnerSelector::Immutable => {
            let prefix = KindPrefix(3);
            page.paginate_filtered(&schema.object_by_owner, &prefix, |_, key, _| {
                matches_type_filter(filter.as_ref(), negated, &key.type_)
            })?
        }
    };

    Ok(grpc::ListObjectsResponse {
        has_previous_page: Some(resp.has_prev),
        has_next_page: Some(resp.has_next),
        objects: resp
            .results
            .into_iter()
            .map(|(token, key, value)| object_proto(key.object_id, value.0, Some(token)))
            .collect(),
    })
}

pub(super) fn list_objects_by_type(
    state: &State,
    checkpoint: u64,
    request: grpc::ListObjectsByTypeRequest,
) -> Result<grpc::ListObjectsResponse, Error> {
    let raw = request
        .object_type
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or(Error::MissingType)?;
    // `list_objects_by_type` is the positive-only variant —
    // negation only makes sense in `list_owned_objects` where
    // we also have a base "everything owned by X" set.
    let (filter, negated) = parse_filter(Some(raw))?;
    if negated {
        return Err(Error::InvalidType(raw.to_owned()));
    }
    let filter = filter.ok_or(Error::MissingType)?;

    let snap = state.snapshot(checkpoint)?;
    let schema = RpcStoreSchema::open(&state.db)?.at(&snap);
    let page = Page::from_request(
        &state.pagination,
        request.after_token(),
        request.before_token(),
        request.page_size(),
        End::from_proto(request.end.unwrap_or_default()),
    );

    let resp = page.paginate_prefix(&schema.object_by_type, &filter)?;

    Ok(grpc::ListObjectsResponse {
        has_previous_page: Some(resp.has_prev),
        has_next_page: Some(resp.has_next),
        objects: resp
            .results
            .into_iter()
            .map(|(token, key, value)| object_proto(key.object_id, value.0, Some(token)))
            .collect(),
    })
}

enum OwnerSelector {
    Address(SuiAddress),
    Object(SuiAddress),
    Shared,
    Immutable,
}

fn parse_owner(owner: &grpc::Owner) -> Result<OwnerSelector, Error> {
    let kind = ProtoOwnerKind::try_from(owner.kind.unwrap_or_default())
        .map_err(|_| Error::InvalidOwnerKind)?;
    match kind {
        ProtoOwnerKind::Address => {
            let s = owner
                .address
                .as_deref()
                .ok_or_else(|| Error::InvalidOwner(String::new()))?;
            Ok(OwnerSelector::Address(parse_address(s)?))
        }
        ProtoOwnerKind::Object => {
            let s = owner
                .address
                .as_deref()
                .ok_or_else(|| Error::InvalidOwner(String::new()))?;
            Ok(OwnerSelector::Object(parse_address(s)?))
        }
        ProtoOwnerKind::Shared => Ok(OwnerSelector::Shared),
        ProtoOwnerKind::Immutable => Ok(OwnerSelector::Immutable),
        ProtoOwnerKind::Unknown => Err(Error::InvalidOwnerKind),
    }
}

fn parse_address(input: &str) -> Result<SuiAddress, Error> {
    parse_sui_address(input).map_err(|_| Error::InvalidOwner(input.to_owned()))
}

fn parse_filter(raw: Option<&str>) -> Result<(Option<TypeFilter>, bool), Error> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok((None, false));
    };
    let (negated, s) = match raw.strip_prefix('!') {
        Some(rest) => (true, rest),
        None => (false, raw),
    };
    if let Ok(tag) = parse_sui_struct_tag(s) {
        return Ok((Some(TypeFilter::Type(tag)), negated));
    }
    if let Ok(module) = parse_sui_module_id(s) {
        return Ok((
            Some(TypeFilter::Module {
                package: SuiAddress::from(*module.address()),
                module: module.name().to_owned(),
            }),
            negated,
        ));
    }
    if let Ok(package) = parse_sui_address(s) {
        return Ok((Some(TypeFilter::Package(package)), negated));
    }
    Err(Error::InvalidType(raw.to_owned()))
}

/// Check whether `tag` matches `filter`. `None` filter matches
/// everything; `negated` inverts the match.
fn matches_type_filter(
    filter: Option<&TypeFilter>,
    negated: bool,
    tag: &move_core_types::language_storage::StructTag,
) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    let matches = filter_matches(filter, tag);
    if negated { !matches } else { matches }
}

fn filter_matches(filter: &TypeFilter, tag: &move_core_types::language_storage::StructTag) -> bool {
    match filter {
        TypeFilter::Package(addr) => SuiAddress::from(tag.address) == *addr,
        TypeFilter::Module { package, module } => {
            SuiAddress::from(tag.address) == *package && tag.module == *module
        }
        TypeFilter::Type(t) if t.type_params.is_empty() => {
            t.address == tag.address && t.module == tag.module && t.name == tag.name
        }
        TypeFilter::Type(t) => t == tag,
    }
}

/// Helper prefix encoder for the `Shared` and `Immutable` owner
/// kinds: their `object_by_owner` key encoding starts with a
/// single tag byte and no owner address, so iterating the entire
/// owner-kind range is just a single-byte prefix.
struct KindPrefix(u8);

impl sui_consistent_store::Encode for KindPrefix {
    fn encode_into<B: bytes::BufMut>(
        &self,
        buf: &mut B,
    ) -> Result<(), sui_consistent_store::error::EncodeError> {
        buf.put_u8(self.0);
        Ok(())
    }
}

fn object_proto(id: ObjectID, version: u64, page_token: Option<Vec<u8>>) -> grpc::Object {
    grpc::Object {
        object_id: Some(id.to_string()),
        version: Some(version),
        digest: None,
        page_token: page_token.map(Into::into),
    }
}
