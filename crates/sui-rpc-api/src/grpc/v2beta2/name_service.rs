// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_name_service::Domain;
use sui_name_service::NameRecord;
use sui_name_service::NameServiceConfig;
use sui_rpc::proto::timestamp_ms_to_proto;
use sui_sdk_types::Address;

use crate::ErrorReason;
use crate::Result;
use crate::RpcError;
use crate::RpcService;
use sui_rpc::proto::google::rpc::bad_request::FieldViolation;
use sui_rpc::proto::sui::rpc::v2beta2::name_service_server::NameService;
use sui_rpc::proto::sui::rpc::v2beta2::LookupNameRequest;
use sui_rpc::proto::sui::rpc::v2beta2::LookupNameResponse;
use sui_rpc::proto::sui::rpc::v2beta2::ReverseLookupNameRequest;
use sui_rpc::proto::sui::rpc::v2beta2::ReverseLookupNameResponse;

#[tonic::async_trait]
impl NameService for RpcService {
    async fn lookup_name(
        &self,
        request: tonic::Request<LookupNameRequest>,
    ) -> Result<tonic::Response<LookupNameResponse>, tonic::Status> {
        lookup_name(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }

    async fn reverse_lookup_name(
        &self,
        request: tonic::Request<ReverseLookupNameRequest>,
    ) -> Result<tonic::Response<ReverseLookupNameResponse>, tonic::Status> {
        reverse_lookup_name(self, request.into_inner())
            .map(tonic::Response::new)
            .map_err(Into::into)
    }
}

fn name_service_config(service: &RpcService) -> Result<NameServiceConfig> {
    match service.chain_id.chain() {
        sui_protocol_config::Chain::Mainnet => Ok(NameServiceConfig::mainnet()),
        sui_protocol_config::Chain::Testnet => Ok(NameServiceConfig::testnet()),
        sui_protocol_config::Chain::Unknown => Err(RpcError::new(
            tonic::Code::Unimplemented,
            "SuiNS not configured for this network",
        )),
    }
}

#[tracing::instrument(skip(service))]
fn lookup_name(service: &RpcService, request: LookupNameRequest) -> Result<LookupNameResponse> {
    let name_service_config = name_service_config(service)?;

    let domain = request
        .name
        .ok_or_else(|| {
            FieldViolation::new(LookupNameRequest::NAME_FIELD.name)
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<Domain>()
        .map_err(|e| {
            FieldViolation::new(LookupNameRequest::NAME_FIELD.name)
                .with_description(format!("invalid domain: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let record_id = name_service_config.record_field_id(&domain);

    let current_timestamp_ms = service.reader.inner().get_latest_checkpoint()?.timestamp_ms;

    let Some(record_object) = service.reader.inner().get_object(&record_id) else {
        return Err(RpcError::not_found());
    };

    let name_record = NameRecord::try_from(record_object)
        .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;

    let is_valid = if !name_record.is_leaf_record() {
        // Handling SLD names & node subdomains is the same (we handle them as `node` records)
        // We check their expiration, and if not expired, return the target address.
        !name_record.is_node_expired(current_timestamp_ms)
    } else {
        // If a record is a leaf record we need to check its parent for expiration.

        // prepare the parent's field id.
        let parent_domain = domain.parent();
        let parent_record_id = name_service_config.record_field_id(&parent_domain);

        // For a leaf record, we check that:
        // 1. The parent is a valid parent for that leaf record
        // 2. The parent is not expired
        if let Some(parent_object) = service.reader.inner().get_object(&parent_record_id) {
            let parent_name_record = NameRecord::try_from(parent_object)
                .map_err(|e| RpcError::new(tonic::Code::Internal, e.to_string()))?;
            parent_name_record.is_valid_leaf_parent(&name_record)
                && !parent_name_record.is_node_expired(current_timestamp_ms)
        } else {
            false
        }
    };

    if is_valid {
        let mut record = sui_rpc::proto::sui::rpc::v2beta2::NameRecord::default();
        record.id = Some(record_id.to_canonical_string(true));
        record.name = Some(domain.to_string());
        record.registration_nft_id = Some(name_record.nft_id.bytes.to_canonical_string(true));
        record.expiration_timestamp =
            Some(timestamp_ms_to_proto(name_record.expiration_timestamp_ms));
        record.target_address = name_record
            .target_address
            .map(|address| address.to_string());
        record.data = name_record
            .data
            .contents
            .into_iter()
            .map(|entry| (entry.key, entry.value))
            .collect();

        let mut response = LookupNameResponse::default();
        response.record = Some(record);
        Ok(response)
    } else {
        Err(RpcError::new(
            tonic::Code::ResourceExhausted,
            "name has expired",
        ))
    }
}

#[tracing::instrument(skip(service))]
fn reverse_lookup_name(
    service: &RpcService,
    request: ReverseLookupNameRequest,
) -> Result<ReverseLookupNameResponse> {
    let name_service_config = name_service_config(service)?;

    let address = request
        .address
        .ok_or_else(|| {
            FieldViolation::new(ReverseLookupNameRequest::ADDRESS_FIELD.name)
                .with_reason(ErrorReason::FieldMissing)
        })?
        .parse::<Address>()
        .map_err(|e| {
            FieldViolation::new(ReverseLookupNameRequest::ADDRESS_FIELD.name)
                .with_description(format!("invalid address: {e}"))
                .with_reason(ErrorReason::FieldInvalid)
        })?;

    let reverse_record_id = name_service_config.reverse_record_field_id(address.as_ref());

    let Some(field_reverse_record_object) = service.reader.inner().get_object(&reverse_record_id)
    else {
        return Err(RpcError::not_found());
    };

    let domain = field_reverse_record_object
        .to_rust::<sui_types::dynamic_field::Field<Address, Domain>>()
        .ok_or_else(|| {
            RpcError::new(
                tonic::Code::Internal,
                format!("Malformed Object {reverse_record_id}"),
            )
        })?
        .value;

    let domain_name = domain.to_string();

    let maybe_record = lookup_name(service, LookupNameRequest::new(&domain_name))?;

    // If looking up the domain returns an empty result, we return an empty result.
    let Some(record) = maybe_record.record else {
        return Err(RpcError::not_found());
    };

    if record.target_address() != address.to_string() {
        return Err(RpcError::not_found());
    }

    Ok(ReverseLookupNameResponse::new(record))
}
