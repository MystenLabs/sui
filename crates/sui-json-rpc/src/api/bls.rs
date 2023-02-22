// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use jsonrpsee_proc_macros::rpc;

use sui_json_rpc_types::{SuiTBlsSignObjectCommitmentType, SuiTBlsSignRandomnessObjectResponse};
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::ObjectID;

#[open_rpc(namespace = "sui", tag = "Threshold BLS API")]
#[rpc(server, client, namespace = "sui")]
pub trait ThresholdBlsApi {
    /// Sign an a Randomness object with threshold BLS.
    /// **Warning**: This API is a work in progress and uses insecure randomness. Please use it for
    /// testing purposes only.
    #[method(name = "tblsSignRandomnessObject")]
    async fn tbls_sign_randomness_object(
        &self,
        /// The object ID.
        object_id: ObjectID,
        /// The way in which the commitment on the object creation should be verified.
        commitment_type: SuiTBlsSignObjectCommitmentType,
    ) -> RpcResult<SuiTBlsSignRandomnessObjectResponse>;
}
