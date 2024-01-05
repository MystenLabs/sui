// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::crypto::BridgeAuthorityPublicKeyBytes;

#[derive(Debug, Clone)]
pub enum BridgeError {
    // The input is not an invalid transaction digest/hash
    InvalidTxHash,
    // The referenced transaction failed
    OriginTxFailed,
    // The referenced transction does not exist
    TxNotFound,
    // Tx is not yet finalized
    TxNotFinalized,
    // No recognized bridge event in specified transaction and event position
    NoBridgeEventsInTxPosition,
    // Found BridgeEvent but not BridgeAction
    BridgeEventNotActionable,
    // Internal Bridge error
    InternalError(String),
    // Authority signature duplication
    AuthoritySignatureDuplication(String),
    // Too many errors when aggregating authority signatures
    AuthoritySignatureAggregationTooManyError(String),
    // Transient Ethereum provider error
    TransientProviderError(String),
    // Ethereum provider error
    ProviderError(String),
    // Invalid BridgeCommittee
    InvalidBridgeCommittee(String),
    // Invalid Bridge authority signature
    InvalidBridgeAuthoritySignature((BridgeAuthorityPublicKeyBytes, String)),
    // Entity is not in the Bridge committee or is blocklisted
    InvalidBridgeAuthority(BridgeAuthorityPublicKeyBytes),
    // Authority's base_url is invalid
    InvalidAuthorityUrl(BridgeAuthorityPublicKeyBytes),
    // Message is signed by mismatched authority
    MismatchedAuthoritySigner,
    // Signature is over a mismatched action
    MismatchedAction,
    // Sui transaction failure due to generic error
    SuiTxFailureGeneric(String),
    // Storage Error
    StorageError(String),
    // Rest API Error
    RestAPIError(String),
    // Uncategorized error
    Generic(String),
}

pub type BridgeResult<T> = Result<T, BridgeError>;
