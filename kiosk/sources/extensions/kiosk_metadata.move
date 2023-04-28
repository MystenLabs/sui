// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This module defines a `kiosk_metadata` extension 
// that can be used to add metadata to a kiosk.
// This metadata can be used to store information
// about the kiosk, such as a name, an image_url or extra key, value pairs.
module kiosk::kiosk_metadata {

    use sui::object::{UID, ID};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use std::string::{String};
    use sui::vec_map::{Self, VecMap};
    use sui::dynamic_field as df;
    use std::vector::{Self};
    use sui::event::{emit as emit_event};

    struct Metadata has store, copy, drop {
        metadata: VecMap<String, String>
    }

    // === Events === 
    // Event that is emmitted when kiosk metadata is created
    struct KioskMetaDataCreated has copy, drop {
        kiosk_id: ID
    }

    // the kiosk & kioskOwnerCap do not match, the user is not the owner of the kiosk.
    const ENotKioskOwner: u64 = 1;
    // invalid length of the fields and values vectors for key,val inserts.
    const EVecLengthMismatch: u64 = 2;
    // trying to remove a key that doesn't exist
    const EKeyNotExists: u64 = 3;

    struct Kiosk_MetadataExt has copy, store, drop {}

    // Create a new Metadata object for the user's Kiosk.
    // Creates a dynamic field object with the Metadata.
    entry public fun enable(kiosk: &mut Kiosk, cap: &KioskOwnerCap){

       // verifies that we have kiosk ownership and returns the kiosk's ID.
        // exits if the user doesn't have write access.
        let mut_kiosk_uid = getWriteableKioskId(kiosk, cap);

        // Create the metadata dynamic field for the owned kiosk.
        df::add(mut_kiosk_uid, Kiosk_MetadataExt{ },  Metadata{ 
            metadata: vec_map::empty(),
        });

        emit_event(KioskMetaDataCreated{ kiosk_id: sui::object::uid_to_inner(mut_kiosk_uid) });
    }

    // destroy the extension from the Kiosk
    entry public fun destroy(kiosk: &mut Kiosk, cap: &KioskOwnerCap){
        // verifies that we have kiosk ownership and returns the kiosk's ID.
        // exits if the user doesn't have write access.
        let mut_kiosk_uid = getWriteableKioskId(kiosk, cap);

        // remove the Metadata from the kiosk.
        let _ : Metadata = df::remove(mut_kiosk_uid, Kiosk_MetadataExt{}); 
    }

    // === Data Modifiers ===
    // 
    // Sets a custom `name` field with the `value`.
    entry public fun add(kiosk: &mut Kiosk, cap: &KioskOwnerCap, name: String, value: String
        ) {

        // gets writeable kiosk Id if it exists, otherwise exits with error.
        let mut_kiosk_uid = getWriteableKioskId(kiosk, cap);
        // gets mutable metadata object if it's registered, otherwise exits with error.
        let metadata = get_mut_metadata(mut_kiosk_uid); 

        add_internal(metadata, name, value)
    }

    // Sets multiple `fields` with `values`.
    entry public fun add_multiple(kiosk: &mut Kiosk, cap: &KioskOwnerCap, fields: vector<String>, values: vector<String>
    ) {
        // gets writeable kiosk Id if it exists, otherwise exits with error.
        let mut_kiosk_uid = getWriteableKioskId(kiosk, cap);

        // gets mutable metadata object if it's registered, otherwise exits with error.
        let metadata = get_mut_metadata(mut_kiosk_uid); 

        let len = vector::length(&fields);
        assert!(len == vector::length(&values), EVecLengthMismatch);

        let i = 0;
        while (i < len) {
            add_internal(metadata, *vector::borrow(&fields, i), *vector::borrow(&values, i));
            i = i + 1;
        };
    }

     // Remove a key from the metadata vec map.
    entry public fun remove_key(kiosk: &mut Kiosk, cap: &KioskOwnerCap, name: String) {

        // gets writeable kiosk Id if it exists, otherwise exits with error.
        let mut_kiosk_uid = getWriteableKioskId(kiosk, cap);

        // gets mutable metadata object if it's registered, otherwise exits with error.
        let metadata = get_mut_metadata(mut_kiosk_uid); 

        // verify that the key we're trying to remove exists on the metadata.
        assert!(vec_map::contains(&metadata.metadata, &name), EKeyNotExists);

        // remove key from map
        vec_map::remove(&mut metadata.metadata, &name);
    }

    // === Access data ==== 
    // Get the metadata for the kiosk
    public fun get_metadata(kiosk_uid: &UID): &Metadata {
        df::borrow(kiosk_uid, Kiosk_MetadataExt{})
    }

    // Access to the metadata VecMap
    public fun get_metadata_vecmap(metadata: &Metadata): &VecMap<String, String>{
        &metadata.metadata
    }

    // === internal helpers ====

    // Internal helper to verify read-access to the kiosk
    // Returns the `kiosk_id` if the user has write access
    // Exits if kioskOwnerCap is invalid (or doesn't match the kiosk object)
    fun getWriteableKioskId(kiosk: &mut Kiosk, cap: &KioskOwnerCap): &mut UID{
        // verify that the user has `admin` access to the kiosk
        assert!(kiosk::has_access(kiosk, cap), ENotKioskOwner);
        // get mutable kiosk uid
        kiosk::uid_mut(kiosk)
    }

    // Private method for inserting fields without security checks.
    fun add_internal(metadata: &mut Metadata, name: String, value: String) {
        vec_map::insert(&mut metadata.metadata, name, value)
    }

    // get mutable metadata
    fun get_mut_metadata(kiosk_uid: &mut UID): &mut Metadata {
        df::borrow_mut(kiosk_uid, Kiosk_MetadataExt{ })
    }
}
