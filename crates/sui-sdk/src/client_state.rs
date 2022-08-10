// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::anyhow;
use move_core_types::language_storage::ModuleId;
use move_core_types::resolver::ModuleResolver;

use sui_json_rpc_types::{GetRawObjectDataResponse, SuiData};
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::move_package::MovePackage;

#[derive(Default)]
pub(crate) struct ClientState {
    objects: BTreeMap<ObjectRef, GetRawObjectDataResponse>,
    object_refs: BTreeMap<ObjectID, ObjectRef>,
}

impl ClientState {
    pub fn get_raw_object(&self, object_id: ObjectID) -> Option<&GetRawObjectDataResponse> {
        self.object_refs
            .get(&object_id)
            .and_then(|oref| self.objects.get(oref))
    }

    pub fn update_object(&mut self, response: GetRawObjectDataResponse) {
        match &response {
            GetRawObjectDataResponse::Exists(o) => {
                let oref = &o.reference;
                self.object_refs
                    .insert(oref.object_id, oref.to_object_ref());
                self.objects.insert(oref.to_object_ref(), response);
            }
            GetRawObjectDataResponse::NotExists(_) => {}
            GetRawObjectDataResponse::Deleted(oref) => {
                self.object_refs
                    .insert(oref.object_id, oref.to_object_ref());
                self.objects.insert(oref.to_object_ref(), response);
            }
        };
    }

    pub fn update_refs(&mut self, orefs: Vec<ObjectRef>) {
        for oref in orefs {
            self.object_refs.insert(oref.0, oref);
        }
    }
}

impl ModuleResolver for ClientState {
    type Error = anyhow::Error;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        let response = self
            .get_raw_object(ObjectID::from(*id.address()))
            .ok_or_else(|| anyhow!("Cannot found package [{}]", id.address()))?;
        let package = response
            .object()?
            .data
            .try_as_package()
            .ok_or_else(|| anyhow!("Object [{}] is not a Move Package.", id.address()))?;
        let package = MovePackage::new(package.id, &package.module_map);
        Ok(package
            .serialized_module_map()
            .get(id.name().as_str())
            .cloned())
    }
}
