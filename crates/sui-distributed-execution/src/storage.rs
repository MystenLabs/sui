use sui_types::{
    base_types::{ObjectID, ObjectRef},
    object::Object,
};

pub trait WritableObjectStore {
    fn insert(&self, 
        k: ObjectID, 
        v: (ObjectRef, Object)
    ) -> Option<(ObjectRef, Object)>;

    fn remove(&self,
         k: ObjectID
    ) -> Option<(ObjectRef, Object)>;
}
