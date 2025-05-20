// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module sui::party;

use sui::vec_map::{Self, VecMap};

/// A party can read the object, taking it as an immutable argument. This restriction is checked
/// when sending the transaction.
const READ: u8 = 0x01;

/// The party can mutate the object, but not change its owner or delete it. This is checked at
/// end end of transaction execution.
const WRITE: u8 = 0x02;

/// The party can delete the object, but not otherwise modify it. This is checked at the end of
/// transaction execution.
const DELETE: u8 = 0x04;

/// The party can change the owner of the object, but not otherwise modify it. This is checked at
/// the end of transaction execution.
const TRANSFER: u8 = 0x08;

/// No permissions.
const NO_PERMISSIONS: u64 = 0;

/// All permissions.
const ALL_PERMISSIONS: u64 = (READ | WRITE | DELETE | TRANSFER) as u64;

/// The permissions that apply to a party object. If the transaction sender has an entry in
/// the `members` map, the permissions in that entry apply. Otherwise, the `default` permissions
/// are used.
/// If the party has the `READ` permission, the object can be taken as an immutable input.
/// If the party has the `WRITE`, `DELETE`, or `TRANSFER` permissions, the object can be taken as
/// a mutable input. Additional restrictions pertaining to each permission are checked at the end
/// of transaction execution.
public struct Party has copy, drop {
    /// The permissions that apply if no specific permissions are set in the `members` map.
    default: Permissions,
    /// The permissions per transaction sender.
    members: VecMap<address, Permissions>,
}

/// The permissions that a party has. The permissions are a bitset of the `READ`, `WRITE`,
/// `DELETE`, and `TRANSFER` constants.
public struct Permissions(u64) has copy, drop;

/// Creates a `Party` value with a single "owner" that has all permissions. No other party
/// has any permissions. And there are no default permissions.
public fun single_owner(owner: address): Party {
    let mut mp = empty();
    mp.set_permissions(owner, Permissions(ALL_PERMISSIONS));
    mp
}

/// A helper `macro` that calls `sui::transfer::party_transfer`.
public macro fun transfer<$T: key>($self: Party, $obj: $T) {
    let mp = $self;
    sui::transfer::party_transfer($obj, mp)
}

/// A helper `macro` that calls `sui::transfer::public_party_transfer`.
public macro fun public_transfer<$T: key + store>($self: Party, $obj: $T) {
    let mp = $self;
    sui::transfer::public_party_transfer($obj, mp)
}

/* public */ fun empty(): Party {
    Party {
        default: Permissions(NO_PERMISSIONS),
        members: vec_map::empty(),
    }
}

/* public */ fun set_permissions(p: &mut Party, address: address, permissions: Permissions) {
    if (p.members.contains(&address)) {
        p.members.remove(&address);
    };
    p.members.insert(address, permissions);
}

public(package) fun is_single_owner(p: &Party): bool {
    p.default.0 == NO_PERMISSIONS &&
    p.members.size() == 1 &&
    { let (_, m) = p.members.get_entry_by_idx(0); m.0 == ALL_PERMISSIONS }
}

public(package) fun into_native(p: Party): (u64, vector<address>, vector<u64>) {
    let Party { default, members } = p;
    let (addresses, permissions) = members.into_keys_values();
    let permissions = permissions.map!(|Permissions(x)| x);
    (default.0, addresses, permissions)
}
