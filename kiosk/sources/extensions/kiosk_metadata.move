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
    use sui::vec_map::{VecMap};
    use sui::dynamic_field as df;

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
    public fun enable(kiosk: &mut Kiosk, cap: &KioskOwnerCap, metadata: VecMap<String, String>){

       // verifies that we have kiosk ownership and returns the kiosk's ID.
        // exits if the user doesn't have write access.
        let kiosk_uid_mut = kiosk::uid_mut_as_owner(kiosk, cap);

        // Create the metadata dynamic field for the owned kiosk.
        df::add(kiosk_uid_mut, KioskMetadataExt{ },  Metadata{ 
            metadata
        });
    }

    // remove the extension from the Kiosk, returns the VecMap that was active.
    public fun remove(kiosk: &mut Kiosk, cap: &KioskOwnerCap): VecMap<String, String>{
        // verifies that we have kiosk ownership and returns the kiosk's ID.
        // exits if the user doesn't have write access.
        let kiosk_uid_mut = kiosk::uid_mut_as_owner(kiosk, cap);

        // remove the Metadata from the kiosk.
        let meta : Metadata = df::remove(kiosk_uid_mut, KioskMetadataExt{}); 
        // return the old metadata VecMap
        meta.metadata
    }

    public fun replace(kiosk: &mut Kiosk, cap: &KioskOwnerCap, metadata: VecMap<String, String>){
        // verifies that we have kiosk ownership and returns the kiosk's ID.
        // exits if the user doesn't have write access.
        let kiosk_uid_mut = kiosk::uid_mut_as_owner(kiosk, cap);
        
        // borrow the metadata obj and replace its contents with the new metadata.
        let meta : &mut Metadata = df::borrow_mut(kiosk_uid_mut, KioskMetadataExt{}); 
        
        meta.metadata = metadata;
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
}
