// Copyright (c) Facebook, Inc. and its affiliates.
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
    Order(Box<Order>),
    Vote(Box<SignedOrder>),
    Cert(Box<CertifiedOrder>),
    Error(Box<FastPayError>),
    InfoReq(Box<InfoRequest>),
    InfoResp(Box<InfoResponse>),
}

// This helper structure is only here to avoid cloning while serializing commands.
// Here we must replicate the definition of SerializedMessage exactly
// so that the variant tags match.
#[derive(Serialize)]
enum ShallowSerializedMessage<'a> {
    Order(&'a Order),
    Vote(&'a SignedOrder),
    Cert(&'a CertifiedOrder),
    Error(&'a FastPayError),
    InfoReq(&'a InfoRequest),
    InfoResp(&'a InfoResponse),
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

pub fn serialize_order(value: &Order) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Order(value))
}

pub fn serialize_transfer_order_into<W>(writer: W, value: &Order) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::Order(value))
}

pub fn serialize_error(value: &FastPayError) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Error(value))
}

pub fn serialize_cert(value: &CertifiedOrder) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Cert(value))
}

pub fn serialize_cert_into<W>(writer: W, value: &CertifiedOrder) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::Cert(value))
}

pub fn serialize_info_request(value: &InfoRequest) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::InfoReq(value))
}

pub fn serialize_info_response(value: &InfoResponse) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::InfoResp(value))
}

pub fn serialize_vote(value: &SignedOrder) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::Vote(value))
}

pub fn serialize_vote_into<W>(writer: W, value: &SignedOrder) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::Vote(value))
}

pub fn deserialize_message<R>(reader: R) -> Result<SerializedMessage, anyhow::Error>
where
    R: std::io::Read,
{
    bincode::deserialize_from(reader).map_err(|err| format_err!("{}", err))
}
