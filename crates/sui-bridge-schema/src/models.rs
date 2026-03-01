// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::data_types::PgTimestamp;
use diesel::deserialize::FromSql;
use diesel::pg::{Pg, PgValue};
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::Text;
use diesel::{AsExpression, FromSqlRow, Identifiable, Insertable, Queryable, Selectable};
use std::str::FromStr;
use strum_macros::{AsRefStr, EnumString};
use sui_field_count::FieldCount;

use crate::schema::{
    governance_actions, progress_store, sui_error_transactions, sui_progress_store, token_transfer,
    token_transfer_data,
};

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = progress_store, primary_key(task_name))]
pub struct ProgressStore {
    pub task_name: String,
    pub checkpoint: i64,
    pub target_checkpoint: i64,
    pub timestamp: Option<PgTimestamp>,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = sui_progress_store, primary_key(txn_digest))]
pub struct SuiProgressStore {
    pub id: i32, // Dummy value
    pub txn_digest: Vec<u8>,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug, FieldCount, Clone)]
#[diesel(table_name = token_transfer, primary_key(chain_id, nonce))]
pub struct TokenTransfer {
    pub chain_id: i32,
    pub nonce: i64,
    pub status: TokenTransferStatus,
    pub block_height: i64,
    pub timestamp_ms: i64,
    pub txn_hash: Vec<u8>,
    pub txn_sender: Vec<u8>,
    pub gas_usage: i64,
    pub data_source: BridgeDataSource,
    pub is_finalized: bool,
}

#[derive(Copy, Clone, Debug, AsExpression, FromSqlRow, EnumString, AsRefStr, PartialEq)]
#[diesel(sql_type = Text)]
pub enum TokenTransferStatus {
    Deposited,
    Approved,
    Claimed,
}

impl FromSql<Text, Pg> for TokenTransferStatus {
    fn from_sql(bytes: PgValue<'_>) -> diesel::deserialize::Result<Self> {
        let s = std::str::from_utf8(bytes.as_bytes())?;
        Ok(TokenTransferStatus::from_str(s)?)
    }
}
impl ToSql<Text, Pg> for TokenTransferStatus {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        <str as ToSql<Text, Pg>>::to_sql(self.as_ref(), out)
    }
}

#[derive(Copy, Clone, Debug, AsExpression, FromSqlRow, EnumString, AsRefStr)]
#[diesel(sql_type = Text)]
pub enum BridgeDataSource {
    SUI,
    ETH,
}

impl FromSql<Text, Pg> for BridgeDataSource {
    fn from_sql(bytes: PgValue<'_>) -> diesel::deserialize::Result<Self> {
        let s = std::str::from_utf8(bytes.as_bytes())?;
        Ok(BridgeDataSource::from_str(s)?)
    }
}

impl ToSql<Text, Pg> for BridgeDataSource {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        <str as ToSql<Text, Pg>>::to_sql(self.as_ref(), out)
    }
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug, FieldCount, Clone)]
#[diesel(table_name = token_transfer_data, primary_key(chain_id, nonce))]
pub struct TokenTransferData {
    pub chain_id: i32,
    pub nonce: i64,
    pub block_height: i64,
    pub timestamp_ms: i64,
    pub txn_hash: Vec<u8>,
    pub sender_address: Vec<u8>,
    pub destination_chain: i32,
    pub recipient_address: Vec<u8>,
    pub token_id: i32,
    pub amount: i64,
    pub is_finalized: bool,
    /// For V2 transfers, the timestamp (in ms) embedded in the bridge message.
    /// NULL for V1 transfers. The frontend uses this to determine limiter-bypass
    /// eligibility: if `now - message_timestamp_ms > 48h`, the transfer bypasses
    /// the rate limiter.
    pub message_timestamp_ms: Option<i64>,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug, FieldCount)]
#[diesel(table_name = sui_error_transactions, primary_key(txn_digest))]
pub struct SuiErrorTransactions {
    pub txn_digest: Vec<u8>,
    pub sender_address: Vec<u8>,
    pub timestamp_ms: i64,
    pub failure_status: String,
    pub cmd_idx: Option<i64>,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug, FieldCount)]
#[diesel(table_name = governance_actions, primary_key(txn_digest))]
pub struct GovernanceAction {
    pub nonce: Option<i64>,
    pub data_source: BridgeDataSource,
    pub txn_digest: Vec<u8>,
    pub sender_address: Vec<u8>,
    pub timestamp_ms: i64,
    pub action: GovernanceActionType,
    pub data: serde_json::Value,
}

#[derive(Copy, Clone, Debug, AsExpression, FromSqlRow, EnumString, AsRefStr)]
#[diesel(sql_type = Text)]
pub enum GovernanceActionType {
    UpdateCommitteeBlocklist,
    EmergencyOperation,
    UpdateBridgeLimit,
    UpdateTokenPrices,
    UpgradeEVMContract,
    AddSuiTokens,
    AddEVMTokens,
}

impl FromSql<Text, Pg> for GovernanceActionType {
    fn from_sql(bytes: PgValue<'_>) -> diesel::deserialize::Result<Self> {
        let s = std::str::from_utf8(bytes.as_bytes())?;
        Ok(GovernanceActionType::from_str(s)?)
    }
}

impl ToSql<Text, Pg> for GovernanceActionType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Pg>) -> diesel::serialize::Result {
        <str as ToSql<Text, Pg>>::to_sql(self.as_ref(), out)
    }
}
