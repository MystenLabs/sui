// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{crypto::BridgeAuthorityPublicKeyBytes, types::BridgeAction};

#[derive(Debug, Clone, PartialEq, Eq)]
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
    // Found a bridge event but not in a recognized Eth bridge contract
    BridgeEventInUnrecognizedEthContract,
    // Found a bridge event but not in a recognized Sui bridge package
    BridgeEventInUnrecognizedSuiPackage,
    // Found BridgeEvent but not BridgeAction
    BridgeEventNotActionable,
    // Failure to serialize
    BridgeSerializationError(String),
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
    // TokenId is unknown
    UnknownTokenId(u8),
    // Invalid BridgeCommittee
    InvalidBridgeCommittee(String),
    // Invalid Bridge authority signature
    InvalidBridgeAuthoritySignature((BridgeAuthorityPublicKeyBytes, String)),
    // Entity is not in the Bridge committee or is blocklisted
    InvalidBridgeAuthority(BridgeAuthorityPublicKeyBytes),
    // Authority's base_url is invalid
    InvalidAuthorityUrl(BridgeAuthorityPublicKeyBytes),
    // Invalid Bridge Client request
    InvalidBridgeClientRequest(String),
    // Invalid ChainId
    InvalidChainId,
    // Message is signed by mismatched authority
    MismatchedAuthoritySigner,
    // Signature is over a mismatched action
    MismatchedAction,
    // Action is not a governance action
    ActionIsNotGovernanceAction(BridgeAction),
    // Client requested a non-approved governace action
    GovernanceActionIsNotApproved,
    // Authority has invalid url
    AuthoirtyUrlInvalid,
    // Action is not token transfer
    ActionIsNotTokenTransferAction,
    // Sui transaction failure due to generic error
    SuiTxFailureGeneric(String),
    // Zero value bridge transfer should not be allowed
    ZeroValueBridgeTransfer(String),
    // Storage Error
    StorageError(String),
    // Rest API Error
    RestAPIError(String),
    // Uncategorized error
    Generic(String),
}

pub type BridgeResult<T> = Result<T, BridgeError>;
