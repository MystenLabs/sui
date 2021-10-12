// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use super::messages::*;
use crate::error::*;

use failure::format_err;
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "unit_tests/serialize_tests.rs"]
mod serialize_tests;

#[derive(Serialize, Deserialize)]
pub enum SerializedMessage {
    Order(Box<TransferOrder>),
    Vote(Box<SignedTransferOrder>),
    Cert(Box<CertifiedTransferOrder>),
    CrossShard(Box<CertifiedTransferOrder>),
    Error(Box<FastPayError>),
    InfoReq(Box<AccountInfoRequest>),
    InfoResp(Box<AccountInfoResponse>),
}

// This helper structure is only here to avoid cloning while serializing commands.
// Here we must replicate the definition of SerializedMessage exactly
// so that the variant tags match.
#[derive(Serialize)]
enum ShallowSerializedMessage<'a> {
    Order(&'a TransferOrder),
    Vote(&'a SignedTransferOrder),
    Cert(&'a CertifiedTransferOrder),
    CrossShard(&'a CertifiedTransferOrder),
    Error(&'a FastPayError),
    InfoReq(&'a AccountInfoRequest),
    InfoResp(&'a AccountInfoResponse),
}

fn serialize_into<T, W>(writer: W, msg: &T) -> Result<(), failure::Error>
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

pub fn serialize_transfer_order(value: &TransferOrder) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Order(value))
}

pub fn serialize_transfer_order_into<W>(
    writer: W,
    value: &TransferOrder,
) -> Result<(), failure::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::Order(value))
}

pub fn serialize_error(value: &FastPayError) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Error(value))
}

pub fn serialize_cert(value: &CertifiedTransferOrder) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Cert(value))
}

pub fn serialize_cert_into<W>(
    writer: W,
    value: &CertifiedTransferOrder,
) -> Result<(), failure::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::Cert(value))
}

pub fn serialize_info_request(value: &AccountInfoRequest) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::InfoReq(value))
}

pub fn serialize_info_response(value: &AccountInfoResponse) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::InfoResp(value))
}

pub fn serialize_cross_shard(value: &CertifiedTransferOrder) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::CrossShard(value))
}

pub fn serialize_vote(value: &SignedTransferOrder) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Vote(value))
}

pub fn serialize_vote_into<W>(writer: W, value: &SignedTransferOrder) -> Result<(), failure::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::Vote(value))
}

pub fn deserialize_message<R>(reader: R) -> Result<SerializedMessage, failure::Error>
where
    R: std::io::Read,
{
    bincode::deserialize_from(reader).map_err(|err| format_err!("{}", err))
}
