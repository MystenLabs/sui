use serde::Deserialize;
use serde::Serialize;

use sui_types::base_types::bytes_as_hex;
use sui_types::base_types::bytes_from_hex;
use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber};

#[derive(Serialize, Deserialize)]
pub struct ObjectResponse {
    pub objects: Vec<NamedObjectRef>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedObjectRef {
    object_id: ObjectID,
    version: SequenceNumber,
    #[serde(serialize_with = "bytes_as_hex", deserialize_with = "bytes_from_hex")]
    digest: ObjectDigest,
}

impl NamedObjectRef {
    pub fn from((object_id, version, digest): ObjectRef) -> Self {
        Self {
            object_id,
            version,
            digest,
        }
    }

    pub fn to_object_ref(self) -> ObjectRef {
        (self.object_id, self.version, self.digest)
    }
}
