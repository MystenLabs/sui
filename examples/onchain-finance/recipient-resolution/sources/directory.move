// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::directory;

use sui::event;
use sui::table::{Self, Table};

#[error]
const EWrongDirectoryCap: vector<u8> = b"Capability does not match this directory";
#[error]
const ENameNotFound: vector<u8> = b"Name not found in directory";

public struct EntryAdded has copy, drop {
    name: vector<u8>,
    address: address,
}

public struct EntryRemoved has copy, drop {
    name: vector<u8>,
}

// docs::#directory
public struct Directory has key {
    id: UID,
    entries: Table<vector<u8>, address>,
}

public struct DirectoryCap has key, store {
    id: UID,
    directory_id: ID,
}

fun init(ctx: &mut TxContext) {
    let uid = object::new(ctx);
    let directory_id = uid.to_inner();
    let directory = Directory {
        id: uid,
        entries: table::new(ctx),
    };
    let cap = DirectoryCap { id: object::new(ctx), directory_id };
    transfer::share_object(directory);
    transfer::public_transfer(cap, ctx.sender());
}

public fun add(dir: &mut Directory, cap: &DirectoryCap, name: vector<u8>, addr: address) {
    assert!(object::id(dir) == cap.directory_id, EWrongDirectoryCap);
    dir.entries.add(name, addr);
    event::emit(EntryAdded { name, address: addr });
}

public fun update(dir: &mut Directory, cap: &DirectoryCap, name: vector<u8>, new_addr: address) {
    assert!(object::id(dir) == cap.directory_id, EWrongDirectoryCap);
    let entry = dir.entries.borrow_mut(name);
    *entry = new_addr;
}

public fun remove(dir: &mut Directory, cap: &DirectoryCap, name: vector<u8>) {
    assert!(object::id(dir) == cap.directory_id, EWrongDirectoryCap);
    dir.entries.remove(name);
    event::emit(EntryRemoved { name });
}

public fun resolve(dir: &Directory, name: vector<u8>): address {
    assert!(dir.entries.contains(name), ENameNotFound);
    *dir.entries.borrow(name)
}
// docs::/#directory
