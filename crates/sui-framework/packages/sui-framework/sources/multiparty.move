// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::multiparty;

use sui::vec_map::{Self, VecMap};

public struct Multiparty has copy, drop {
    global: Permissions,
    parties: VecMap<address, Permissions>,
}

public struct Permissions(u64) has copy, drop;

const READ: u8 = 0x01;
const WRITE: u8 = 0x02;
const DELETE: u8 = 0x04;
const TRANSFER: u8 = 0x08;

public fun single_owner(owner: address): Multiparty {
    let mut mp = empty();
    mp.set_permissions(owner, all_permissions());
    mp
}

/* public */ fun empty(): Multiparty {
    Multiparty {
        global: no_permissions!(),
        parties: vec_map::empty(),
    }
}

/* public */ fun set_permissions(m: &mut Multiparty, address: address, permissions: Permissions) {
    if (m.parties.contains(&address)) {
        m.parties.remove(&address);
    };
    m.parties.insert(address, permissions);

}

/* public */ fun all_permissions(): Permissions {
    Permissions((READ | WRITE | DELETE | TRANSFER) as u64)
}

macro fun no_permissions(): Permissions {
    Permissions(0)
}

public(package) fun is_single_owner(m: &Multiparty): bool {
    m.global == no_permissions!() &&
    m.parties.size() == 1 &&
    { let (_, p) = m.parties.get_entry_by_idx(0); p == all_permissions() }
}
