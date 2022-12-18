// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::ThresholdBlsApiServer;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use fastcrypto_tbls::{mocked_dkg, tbls::ThresholdBls, types::ThresholdBls12381MinSig};
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::SuiTBlsSignObjectCreationEpoch::{CurrentEpoch, PriorEpoch};
use sui_json_rpc_types::{SuiTBlsSignObjectCreationEpoch, SuiTBlsSignRandomnessObjectResponse};
use sui_open_rpc::Module;
use sui_types::base_types::ObjectID;
use sui_types::crypto::AuthoritySignInfoTrait;

pub struct ThresholdBlsApiImpl {
    state: Arc<AuthorityState>,
}

impl ThresholdBlsApiImpl {
    pub fn new(state: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ThresholdBlsApiServer for ThresholdBlsApiImpl {
    async fn tbls_sign_randomness_object(
        &self,
        object_id: ObjectID,
        object_creation_epoch: SuiTBlsSignObjectCreationEpoch,
    ) -> RpcResult<SuiTBlsSignRandomnessObjectResponse> {
        let curr_epoch = self.state.epoch();

        // Check that the object is from an old epoch or that it was committed.
        let committed_epoch = match object_creation_epoch {
            // Just make sure we are indeed trying to sign on an old epoch.
            PriorEpoch(prior_epoch) => {
                if prior_epoch >= curr_epoch {
                    Err(anyhow!("Provided prior epoch is not old"))?
                };
                prior_epoch
            }
            // Check that the certificate is valid, for the current epoch, and includes the object.
            CurrentEpoch(effects_cert) => {
                if effects_cert.auth_sign_info.epoch != curr_epoch {
                    Err(anyhow!("Inconsistent epochs"))?
                }
                let committee = self
                    .state
                    .committee_store()
                    .get_committee(&curr_epoch)
                    .map_err(|e| anyhow!(e))?
                    .ok_or(anyhow!("Committee not available"))?; // Should never happen?

                // TODO: convert SuiTransactionEffects to TransactionEffects before the next line
                // effects_cert
                //     .auth_sign_info
                //     .verify(&effects_cert.effects, &committee)
                //     .map_err(|e| anyhow!(e))?;

                effects_cert
                    .effects
                    .created
                    .iter()
                    .chain(effects_cert.effects.mutated.iter())
                    .find(|owned_obj_ref| owned_obj_ref.reference.object_id == object_id)
                    .ok_or(anyhow!("Object was not created/mutated in the provided transaction effects certificate"))?;
                curr_epoch
            }
        };

        // Note that we do not try to get the object from the store since it might not exist on the
        // node.

        // Construct the message to be signed, as done in the Move code of the Randomness object.
        let domain = "randomness".as_bytes();
        let mut msg = domain.to_vec();
        msg.append(&mut u64::to_be_bytes(curr_epoch as u64).to_vec());
        msg.append(&mut object_id.to_vec());

        let (sk, _pk) = mocked_dkg::generate_full_key_pair(committed_epoch);
        let signature = ThresholdBls12381MinSig::sign(&sk, msg.as_slice());
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
