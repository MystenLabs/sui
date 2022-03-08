// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::messages::*;
use crate::error::*;

use anyhow::format_err;
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "unit_tests/serialize_tests.rs"]
mod serialize_tests;

#[derive(Serialize, Deserialize)]
pub enum SerializedMessage {
    Transaction(Box<Transaction>),
    Vote(Box<SignedTransaction>),
    Cert(Box<CertifiedTransaction>),
    Error(Box<SuiError>),
    AccountInfoReq(Box<AccountInfoRequest>),
    AccountInfoResp(Box<AccountInfoResponse>),
    ObjectInfoReq(Box<ObjectInfoRequest>),
    ObjectInfoResp(Box<ObjectInfoResponse>),
    TransactionResp(Box<TransactionInfoResponse>),
    TransactionInfoReq(Box<TransactionInfoRequest>),
    BatchInfoReq(Box<BatchInfoRequest>),
    BatchInfoResp(Box<BatchInfoResponseItem>),
}

// This helper structure is only here to avoid cloning while serializing commands.
// Here we must replicate the definition of SerializedMessage exactly
// so that the variant tags match.
#[allow(dead_code)]
#[derive(Serialize)]
enum ShallowSerializedMessage<'a> {
    Transaction(&'a Transaction),
    Vote(&'a SignedTransaction),
    Cert(&'a CertifiedTransaction),
    Error(&'a SuiError),
    AccountInfoReq(&'a AccountInfoRequest),
    AccountInfoResp(&'a AccountInfoResponse),
    ObjectInfoReq(&'a ObjectInfoRequest),
    ObjectInfoResp(&'a ObjectInfoResponse),
    TransactionResp(&'a TransactionInfoResponse),
    TransactionInfoReq(&'a TransactionInfoRequest),
    BatchInfoReq(&'a BatchInfoRequest),
    BatchInfoResp(&'a BatchInfoResponseItem),
}

fn serialize_into<T, W>(writer: W, msg: &T) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
    T: Serialize,
{
    bincode::serialize_into(writer, msg).map_err(|err| format_err!("{}", err))
}

fn serialize<T>(msg: &T) -> Vec<u8>
where
    T: Serialize,
{
    let mut buf = Vec::new();
    bincode::serialize_into(&mut buf, msg)
        .expect("Serializing to a resizable buffer should not fail.");
    buf
}

pub fn serialize_message(msg: &SerializedMessage) -> Vec<u8> {
    serialize(msg)
}

pub fn serialize_transaction(value: &Transaction) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Transaction(value))
}

pub fn serialize_transfer_transaction_into<W>(
    writer: W,
    value: &Transaction,
) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::Transaction(value))
}

pub fn serialize_error(value: &SuiError) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Error(value))
}

pub fn serialize_cert(value: &CertifiedTransaction) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Cert(value))
}

pub fn serialize_cert_into<W>(writer: W, value: &CertifiedTransaction) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::Cert(value))
}

pub fn serialize_account_info_request(value: &AccountInfoRequest) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::AccountInfoReq(value))
}

pub fn serialize_account_info_response(value: &AccountInfoResponse) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::AccountInfoResp(value))
}

pub fn serialize_object_info_request(value: &ObjectInfoRequest) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::ObjectInfoReq(value))
}

pub fn serialize_object_info_response(value: &ObjectInfoResponse) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::ObjectInfoResp(value))
}

pub fn serialize_transaction_info_request(value: &TransactionInfoRequest) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::TransactionInfoReq(value))
}

pub fn serialize_vote(value: &SignedTransaction) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Vote(value))
}

pub fn serialize_batch_request(request: &BatchInfoRequest) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::BatchInfoReq(request))
}

pub fn serialize_batch_item(item: &BatchInfoResponseItem) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::BatchInfoResp(item))
}

pub fn serialize_vote_into<W>(writer: W, value: &SignedTransaction) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::Vote(value))
}

pub fn serialize_transaction_info(value: &TransactionInfoResponse) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::TransactionResp(value))
}

pub fn serialize_transaction_info_into<W>(
    writer: W,
    value: &TransactionInfoResponse,
) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::TransactionResp(value))
}

pub fn deserialize_message<R>(reader: R) -> Result<SerializedMessage, anyhow::Error>
where
    R: std::io::Read,
{
    bincode::deserialize_from(reader).map_err(|err| format_err!("{}", err))
}

pub fn deserialize_object_info(message: SerializedMessage) -> Result<ObjectInfoResponse, SuiError> {
    match message {
        SerializedMessage::ObjectInfoResp(resp) => Ok(*resp),
        SerializedMessage::Error(error) => Err(*error),
        _ => Err(SuiError::UnexpectedMessage),
    }
}

pub fn deserialize_account_info(
    message: SerializedMessage,
) -> Result<AccountInfoResponse, SuiError> {
    match message {
        SerializedMessage::AccountInfoResp(resp) => Ok(*resp),
        SerializedMessage::Error(error) => Err(*error),
        _ => Err(SuiError::UnexpectedMessage),
    }
}

pub fn deserialize_transaction_info(
    message: SerializedMessage,
) -> Result<TransactionInfoResponse, SuiError> {
    match message {
        SerializedMessage::TransactionResp(resp) => Ok(*resp),
        SerializedMessage::Error(error) => Err(*error),
        _ => Err(SuiError::UnexpectedMessage),
    }
}
