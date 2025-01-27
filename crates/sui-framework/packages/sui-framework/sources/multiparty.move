// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::multiparty;

public struct Multiparty has copy, drop {
    global: Permissions,
    parties: vector<Party>,
}

public struct Party has copy, drop {
    address: vector<address>,
    flags: Permissions,
}

public struct Permissions(u64) has copy, drop;

const READ: u8 = 0x01;
const WRITE: u8 = 0x02;
const DELETE: u8 = 0x04;
const TRANSFER: u8 = 0x08;

public fun single_owner(owner: address): Multiparty {
    let mut mp = empty();
    mp.add_party(party(vector[owner], all_permissions()));
    mp
}

/* public */ fun empty(): Multiparty {
    Multiparty {
        global: no_permissions!(),
        parties: vector[],
    }
}

/* public */ fun add_party(m: &mut Multiparty, p: Party) {
    m.parties.push_back(p);
}

/* public */ fun party(address: vector<address>, flags: Permissions): Party {
    Party {
        address,
        flags,
    }
}

/* public */ fun all_permissions(): Permissions {
    Permissions((READ | WRITE | DELETE | TRANSFER) as u64)
}

macro fun no_permissions(): Permissions {
    Permissions(0)
}

public(package) fun is_single_owner(m: &Multiparty): bool {
    m.global == no_permissions!() &&
    m.parties.length() == 1 &&
    m.parties[0].flags == all_permissions()
}
