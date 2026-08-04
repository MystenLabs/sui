// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::directory;

use std::string::String;
use sui::event;
use sui::table::{Self, Table};

#[error(code = 0)]
const EWrongDirectoryCap: vector<u8> = b"Capability does not match this directory";
#[error(code = 1)]
const ENameNotFound: vector<u8> = b"Name not found in directory";
#[error(code = 2)]
const ENameAlreadyRegistered: vector<u8> = b"Name is already registered";

public struct EntryAdded has copy, drop {
    directory_id: ID,
    name: String,
    address: address,
}

public struct EntryUpdated has copy, drop {
    directory_id: ID,
    name: String,
    address: address,
}

public struct EntryRemoved has copy, drop {
    directory_id: ID,
    name: String,
}

// docs::#directory
/// A single-operator directory with a capability bound to one shared registry.
public struct Directory has key {
    id: UID,
    entries: Table<String, address>,
}

public struct DirectoryCap has key, store {
    id: UID,
    directory_id: ID,
}

fun init(ctx: &mut TxContext) {
    let directory = Directory {
        id: object::new(ctx),
        entries: table::new(ctx),
    };
    let cap = DirectoryCap {
        id: object::new(ctx),
        directory_id: object::id(&directory),
    };
    transfer::share_object(directory);
    transfer::public_transfer(cap, ctx.sender());
}

public fun add(directory: &mut Directory, cap: &DirectoryCap, name: String, addr: address) {
    assert!(object::id(directory) == cap.directory_id, EWrongDirectoryCap);
    assert!(!directory.entries.contains(name), ENameAlreadyRegistered);
    directory.entries.add(name, addr);
    event::emit(EntryAdded { directory_id: cap.directory_id, name, address: addr });
}

public fun update(directory: &mut Directory, cap: &DirectoryCap, name: String, addr: address) {
    assert!(object::id(directory) == cap.directory_id, EWrongDirectoryCap);
    assert!(directory.entries.contains(name), ENameNotFound);
    *directory.entries.borrow_mut(name) = addr;
    event::emit(EntryUpdated { directory_id: cap.directory_id, name, address: addr });
}

public fun remove(directory: &mut Directory, cap: &DirectoryCap, name: String) {
    assert!(object::id(directory) == cap.directory_id, EWrongDirectoryCap);
    assert!(directory.entries.contains(name), ENameNotFound);
    directory.entries.remove(name);
    event::emit(EntryRemoved { directory_id: cap.directory_id, name });
}

public fun resolve(directory: &Directory, name: String): address {
    assert!(directory.entries.contains(name), ENameNotFound);
    *directory.entries.borrow(name)
}
// docs::/#directory
