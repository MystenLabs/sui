// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::directory;

use sui::event;
use sui::table::{Self, Table};

#[error]
const ENotOwner: vector<u8> = b"Only the directory owner can modify entries";
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
    owner: address,
    entries: Table<vector<u8>, address>,
}

public struct DirectoryCap has key, store {
    id: UID,
}

fun init(ctx: &mut TxContext) {
    let directory = Directory {
        id: object::new(ctx),
        owner: ctx.sender(),
        entries: table::new(ctx),
    };
    let cap = DirectoryCap { id: object::new(ctx) };
    transfer::share_object(directory);
    transfer::public_transfer(cap, ctx.sender());
}

public fun add(dir: &mut Directory, _cap: &DirectoryCap, name: vector<u8>, addr: address) {
    dir.entries.add(name, addr);
    event::emit(EntryAdded { name, address: addr });
}

public fun update(dir: &mut Directory, _cap: &DirectoryCap, name: vector<u8>, new_addr: address) {
    let entry = dir.entries.borrow_mut(&name);
    *entry = new_addr;
}

public fun remove(dir: &mut Directory, _cap: &DirectoryCap, name: vector<u8>) {
    dir.entries.remove(&name);
    event::emit(EntryRemoved { name });
}

public fun resolve(dir: &Directory, name: &vector<u8>): address {
    assert!(dir.entries.contains(name), ENameNotFound);
    *dir.entries.borrow(name)
}
// docs::/#directory
