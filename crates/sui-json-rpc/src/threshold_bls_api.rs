// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::ThresholdBlsApiServer;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use fastcrypto_tbls::{mocked_dkg, tbls::ThresholdBls, types::ThresholdBls12381MinSig};
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use move_core_types::value::MoveStructLayout;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::SuiTBlsSignObjectCommitmentType::{ConsensusCommitted, FastPathCommitted};
use sui_json_rpc_types::{
    SuiCertifiedTransactionEffects, SuiTBlsSignObjectCommitmentType,
    SuiTBlsSignRandomnessObjectResponse,
};
use sui_open_rpc::Module;
use sui_types::base_types::ObjectID;
use sui_types::crypto::construct_tbls_randomness_object_message;
use sui_types::object::{Object, ObjectRead};

pub struct ThresholdBlsApiImpl {
    state: Arc<AuthorityState>,
}

impl ThresholdBlsApiImpl {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }

    /// Check that the given layout represents a Randomness object.
    fn is_randomness_object(layout: &MoveStructLayout) -> bool {
        let MoveStructLayout::WithTypes{type_, fields: _} = layout else { return false; };
        let prefix = "0000000000000000000000000000000000000002::randomness::Randomness";
        type_.to_canonical_string().starts_with(prefix)
    }

    /// Get the object and check if it is a Randomness object.
    async fn get_randomness_object(&self, object_id: ObjectID) -> RpcResult<Object> {
        let obj_read = self
            .state
            .get_object_read(&object_id)
            .await
            .map_err(|e| anyhow!(e))?;
        let ObjectRead::Exists(_obj_ref, obj, layout) = obj_read else {
            Err(anyhow!("Object not found"))? };
        let Some(layout) = layout else { Err(anyhow!("Object does not have a layout"))? };
        if !Self::is_randomness_object(&layout) {
            Err(anyhow!("Not a Randomness object"))?
        }
        Ok(obj)
    }

    /// Return true if the given object exists according to my local view.
    ///
    /// Currently only checks if the object exists in the local storage, but in the future
    /// validators will verify that the object had been created in a transaction that was committed.
    async fn verify_object_alive_and_committed(&self, object_id: ObjectID) -> RpcResult<()> {
        let _obj = self.get_randomness_object(object_id).await?;
        Ok(())
    }

    async fn verify_effects_cert(
        &self,
        object_id: ObjectID,
        effects_cert: &SuiCertifiedTransactionEffects,
    ) -> RpcResult<()> {
        if effects_cert.auth_sign_info.epoch != self.state.epoch() {
            Err(anyhow!(
                "Old effects certificate, check instead if committed by consensus"
            ))?
        }
        // Check the certificate.
        let _committee = self
            .state
            .committee_store()
            .get_committee(&self.state.epoch())
            .map_err(|e| anyhow!(e))?
            .ok_or_else(|| anyhow!("Committee not available"))?;

        // TODO: convert SuiTransactionEffects to TransactionEffects before the next line.
        // effects_cert
        //     .auth_sign_info
        //     .verify(&effects_cert.effects, &committee)
        //     .map_err(|e| anyhow!(e))?;

        // Check that the object is indeed in the effects.
        effects_cert
            .effects
            .created
            .iter()
            .chain(effects_cert.effects.mutated.iter())
            .find(|owned_obj_ref| owned_obj_ref.reference.object_id == object_id)
            .ok_or_else(|| {
                anyhow!("Object was not created/mutated in the provided effects certificate")
            })?;

        // Check that the object is indeed a Randomness object.
        let _obj = self.get_randomness_object(object_id).await?;

        Ok(())
    }
}

#[async_trait]
impl ThresholdBlsApiServer for ThresholdBlsApiImpl {
    /// Currently this is an insecure implementation since we do not have the DKG yet.
    /// All the checks below are done with the local view of the node. Later on those checks will be
    /// done by each of the validators (using their local view) when they are requested to sign
    /// on a Randomness object.
    async fn tbls_sign_randomness_object(
        &self,
        object_id: ObjectID,
        commitment_type: SuiTBlsSignObjectCommitmentType,
    ) -> RpcResult<SuiTBlsSignRandomnessObjectResponse> {
        match commitment_type {
            ConsensusCommitted => self.verify_object_alive_and_committed(object_id).await?,
            FastPathCommitted(effects_cert) => {
                self.verify_effects_cert(object_id, &effects_cert).await?
            }
        };
        // Construct the message to be signed, as done in the Move code of the Randomness object.
        let curr_epoch = self.state.epoch();
        let msg = construct_tbls_randomness_object_message(curr_epoch, &object_id);
        // Sign the message using the mocked DKG keys.
        let (sk, _pk) = mocked_dkg::generate_full_key_pair(curr_epoch);
        let signature = (&ThresholdBls12381MinSig::sign(&sk, msg.as_slice())).into();
        Ok(SuiTBlsSignRandomnessObjectResponse { signature })
    }
}

impl SuiRpcModule for ThresholdBlsApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::ThresholdBlsApiOpenRpc::module_doc()
    }
}
