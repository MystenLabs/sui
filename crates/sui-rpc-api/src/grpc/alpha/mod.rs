// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod event_service;
pub mod get_object_inclusion_proof;
pub mod list_authenticated_events;

pub mod event_service_proto {
    include!("../../proto/generated/sui.rpc.alpha.rs");
}
