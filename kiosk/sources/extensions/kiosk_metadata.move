// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This module defines a `kiosk_metadata` extension 
// that can be used to add metadata to a kiosk.
// This metadata can be used to store information
// about the kiosk, such as a name, an image_url or extra key, value pairs.
module kiosk::kiosk_metadata {

    use sui::object::{UID};
    use sui::kiosk::{Self, Kiosk, KioskOwnerCap};
    use std::string::{String};
    use sui::vec_map::{Self, VecMap};
    use sui::dynamic_field as df;
    use std::vector::{Self};

    struct Metadata has store, copy, drop {
        metadata: VecMap<String, String>
    }

    // invalid length of the fields and values vectors for key,val inserts.
    const EVecLengthMismatch: u64 = 1;
    // trying to remove a key that doesn't exist
    const EKeyNotExists: u64 = 2;

    struct KioskMetadataExt has copy, store, drop {}

    // Create a new Metadata object for the user's Kiosk.
    // Creates a dynamic field object with the Metadata.
    public fun enable(kiosk: &mut Kiosk, cap: &KioskOwnerCap){

       // verifies that we have kiosk ownership and returns the kiosk's ID.
        // exits if the user doesn't have write access.
        let kiosk_uid_mut = kiosk::uid_mut_as_owner(kiosk, cap);

        // Create the metadata dynamic field for the owned kiosk.
        df::add(kiosk_uid_mut, KioskMetadataExt{ },  Metadata{ 
            metadata: vec_map::empty(),
        });
    }

    // remove the extension from the Kiosk, returns the VecMap that was active.
    public fun remove(kiosk: &mut Kiosk, cap: &KioskOwnerCap): VecMap<String, String>{
        // verifies that we have kiosk ownership and returns the kiosk's ID.
        // exits if the user doesn't have write access.
        let kiosk_uid_mut = kiosk::uid_mut_as_owner(kiosk, cap);

        // remove the Metadata from the kiosk.
        let meta : Metadata = df::remove(kiosk_uid_mut, KioskMetadataExt{}); 
        meta.metadata
    }

    // === Data Modifiers ===
    // 
    // Sets a custom `name` field with the `value`.
    public fun add(kiosk: &mut Kiosk, cap: &KioskOwnerCap, name: String, value: String) {
        // gets writeable kiosk Id if it exists, otherwise exits with error.
        let kiosk_uid_mut = kiosk::uid_mut_as_owner(kiosk, cap);
        // gets mutable metadata object if it's registered, otherwise exits with error.
        let metadata = get_mut_metadata(kiosk_uid_mut); 

        add_internal(metadata, name, value)
    }

    // Sets multiple `fields` with `values`.
    public fun add_multiple(kiosk: &mut Kiosk, cap: &KioskOwnerCap, fields: vector<String>, values: vector<String>) {
        // gets writeable kiosk Id if it exists, otherwise exits with error.
        let kiosk_uid_mut = kiosk::uid_mut_as_owner(kiosk, cap);

        // gets mutable metadata object if it's registered, otherwise exits with error.
        let metadata = get_mut_metadata(kiosk_uid_mut); 

        let len = vector::length(&fields);
        assert!(len == vector::length(&values), EVecLengthMismatch);

        let i = 0;
        while (i < len) {
            add_internal(metadata, *vector::borrow(&fields, i), *vector::borrow(&values, i));
            i = i + 1;
        };
    }

     // Remove a key from the metadata vec map.
    public fun remove_field(kiosk: &mut Kiosk, cap: &KioskOwnerCap, name: String) {

        // gets writeable kiosk Id if it exists, otherwise exits with error.
        let kiosk_uid_mut = kiosk::uid_mut_as_owner(kiosk, cap);

        // gets mutable metadata object if it's registered, otherwise exits with error.
        let metadata = get_mut_metadata(kiosk_uid_mut); 

        // verify that the key we're trying to remove exists on the metadata.
        assert!(vec_map::contains(&metadata.metadata, &name), EKeyNotExists);

        // remove key from map
        vec_map::remove(&mut metadata.metadata, &name);
    }

    // === Access data ==== 
    // Get the metadata for the kiosk
    public fun get_metadata(kiosk_uid: &UID): &Metadata {
        df::borrow(kiosk_uid, KioskMetadataExt{})
    }

    // Access to the metadata VecMap
    public fun get_metadata_vecmap(metadata: &Metadata): &VecMap<String, String>{
        &metadata.metadata
    }

    // === internal helpers ====

    // Private method for inserting fields without security checks.
    fun add_internal(metadata: &mut Metadata, name: String, value: String) {
        vec_map::insert(&mut metadata.metadata, name, value)
    }

    // get mutable metadata
    fun get_mut_metadata(kiosk_uid: &mut UID): &mut Metadata {
        df::borrow_mut(kiosk_uid, KioskMetadataExt{ })
    }
}
