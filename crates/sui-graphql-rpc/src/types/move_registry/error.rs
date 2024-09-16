// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use sui_types::base_types::ObjectID;

#[derive(thiserror::Error, Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum MoveRegistryError {
    // The chain identifier is not available, so we cannot determine where to look for the name.
    #[error("Move Registry: Cannot determine which chain to query due to an internal error.")]
    ChainIdentifierUnavailable,
    // The name was found in the service, but it is not a valid name.
    #[error("Move Registry: The request name {0} is malformed.")]
    InvalidName(String),

    #[error("Move Registry: External API url is not available so resolution is not on this RPC.")]
    ExternalApiUrlUnavailable,

    #[error(
        "Move Registry: Internal Error, failed to query external API due to an internal error: {0}"
    )]
    FailedToQueryExternalApi(String),

    #[error("Move Registry Internal Error: Failed to parse external API's response: {0}")]
    FailedToParseExternalResponse(String),

    #[error("Move Registry Internal Error: Failed to deserialize record ${0}.")]
    FailedToDeserializeRecord(ObjectID),

    #[error("Move Registry: The name {0} was not found.")]
    NameNotFound(String),

    #[error("Move Registry: Invalid version")]
    InvalidVersion,
}
