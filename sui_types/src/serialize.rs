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
    Order(Box<Order>),
    Vote(Box<SignedOrder>),
    Cert(Box<CertifiedOrder>),
    Error(Box<SuiError>),
    AccountInfoReq(Box<AccountInfoRequest>),
    AccountInfoResp(Box<AccountInfoResponse>),
    ObjectInfoReq(Box<ObjectInfoRequest>),
    ObjectInfoResp(Box<ObjectInfoResponse>),
    OrderResp(Box<OrderInfoResponse>),
    OrderInfoReq(Box<OrderInfoRequest>),
}

// This helper structure is only here to avoid cloning while serializing commands.
// Here we must replicate the definition of SerializedMessage exactly
// so that the variant tags match.
#[allow(dead_code)]
#[derive(Serialize)]
enum ShallowSerializedMessage<'a> {
    Order(&'a Order),
    Vote(&'a SignedOrder),
    Cert(&'a CertifiedOrder),
    Error(&'a SuiError),
    AccountInfoReq(&'a AccountInfoRequest),
    AccountInfoResp(&'a AccountInfoResponse),
    ObjectInfoReq(&'a ObjectInfoRequest),
    ObjectInfoResp(&'a ObjectInfoResponse),
    OrderResp(&'a OrderInfoResponse),
    OrderInfoReq(&'a OrderInfoRequest),
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

pub fn serialize_error(value: &SuiError) -> Vec<u8> {
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

pub fn serialize_order_info_request(value: &OrderInfoRequest) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::OrderInfoReq(value))
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

pub fn serialize_order_info(value: &OrderInfoResponse) -> Vec<u8> {
    serialize(&ShallowSerializedMessage::OrderResp(value))
}

pub fn serialize_order_info_into<W>(
    writer: W,
    value: &OrderInfoResponse,
) -> Result<(), anyhow::Error>
where
    W: std::io::Write,
{
    serialize_into(writer, &ShallowSerializedMessage::OrderResp(value))
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

pub fn deserialize_order_info(message: SerializedMessage) -> Result<OrderInfoResponse, SuiError> {
    match message {
        SerializedMessage::OrderResp(resp) => Ok(*resp),
        SerializedMessage::Error(error) => Err(*error),
        _ => Err(SuiError::UnexpectedMessage),
    }
}
