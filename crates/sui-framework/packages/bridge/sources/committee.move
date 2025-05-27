// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_use)]
module bridge::committee;

use bridge::crypto;
use bridge::message::{Self, Blocklist, BridgeMessage};
use sui::ecdsa_k1;
use sui::event::emit;
use sui::vec_map::{Self, VecMap};
use sui::vec_set;
use sui_system::sui_system::SuiSystemState;

const ESignatureBelowThreshold: u64 = 0;
const EDuplicatedSignature: u64 = 1;
const EInvalidSignature: u64 = 2;
const ENotSystemAddress: u64 = 3;
const EValidatorBlocklistContainsUnknownKey: u64 = 4;
const ESenderNotActiveValidator: u64 = 5;
const EInvalidPubkeyLength: u64 = 6;
const ECommitteeAlreadyInitiated: u64 = 7;
const EDuplicatePubkey: u64 = 8;
const ESenderIsNotInBridgeCommittee: u64 = 9;

const SUI_MESSAGE_PREFIX: vector<u8> = b"SUI_BRIDGE_MESSAGE";

const ECDSA_COMPRESSED_PUBKEY_LENGTH: u64 = 33;

//////////////////////////////////////////////////////
// Types
//

public struct BlocklistValidatorEvent has copy, drop {
    blocklisted: bool,
    public_keys: vector<vector<u8>>,
}

public struct BridgeCommittee has store {
    // commitee pub key and weight
    members: VecMap<vector<u8>, CommitteeMember>,
    // Committee member registrations for the next committee creation.
    member_registrations: VecMap<address, CommitteeMemberRegistration>,
    // Epoch when the current committee was updated,
    // the voting power for each of the committee members are snapshot from this epoch.
    // This is mainly for verification/auditing purposes, it might not be useful for bridge operations.
    last_committee_update_epoch: u64,
}

public struct CommitteeUpdateEvent has copy, drop {
    // commitee pub key and weight
    members: VecMap<vector<u8>, CommitteeMember>,
    stake_participation_percentage: u64,
}

public struct CommitteeMemberUrlUpdateEvent has copy, drop {
    member: vector<u8>,
    new_url: vector<u8>,
}

public struct CommitteeMember has copy, drop, store {
    /// The Sui Address of the validator
    sui_address: address,
    /// The public key bytes of the bridge key
    bridge_pubkey_bytes: vector<u8>,
    /// Voting power, values are voting power in the scale of 10000.
    voting_power: u64,
    /// The HTTP REST URL the member's node listens to
    /// it looks like b'https://127.0.0.1:9191'
    http_rest_url: vector<u8>,
    /// If this member is blocklisted
    blocklisted: bool,
}

public struct CommitteeMemberRegistration has copy, drop, store {
    /// The Sui Address of the validator
    sui_address: address,
    /// The public key bytes of the bridge key
    bridge_pubkey_bytes: vector<u8>,
    /// The HTTP REST URL the member's node listens to
    /// it looks like b'https://127.0.0.1:9191'
    http_rest_url: vector<u8>,
}

//////////////////////////////////////////////////////
// Public functions
//

public fun verify_signatures(
    self: &BridgeCommittee,
    message: BridgeMessage,
    signatures: vector<vector<u8>>,
) {
    let (mut i, signature_counts) = (0, vector::length(&signatures));
    let mut seen_pub_key = vec_set::empty<vector<u8>>();
    let required_voting_power = message.required_voting_power();
    // add prefix to the message bytes
    let mut message_bytes = SUI_MESSAGE_PREFIX;
    message_bytes.append(message.serialize_message());

    let mut threshold = 0;
    while (i < signature_counts) {
        let pubkey = ecdsa_k1::secp256k1_ecrecover(&signatures[i], &message_bytes, 0);

        // check duplicate
        // and make sure pub key is part of the committee
        assert!(!seen_pub_key.contains(&pubkey), EDuplicatedSignature);
        assert!(self.members.contains(&pubkey), EInvalidSignature);

        // get committee signature weight and check pubkey is part of the committee
        let member = &self.members[&pubkey];
        if (!member.blocklisted) {
            threshold = threshold + member.voting_power;
        };
        seen_pub_key.insert(pubkey);
        i = i + 1;
    };

    assert!(threshold >= required_voting_power, ESignatureBelowThreshold);
}

//////////////////////////////////////////////////////
// Internal functions
//

public(package) fun create(ctx: &TxContext): BridgeCommittee {
    assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);
    BridgeCommittee {
        members: vec_map::empty(),
        member_registrations: vec_map::empty(),
        last_committee_update_epoch: 0,
    }
}

public(package) fun register(
    self: &mut BridgeCommittee,
    system_state: &mut SuiSystemState,
    bridge_pubkey_bytes: vector<u8>,
    http_rest_url: vector<u8>,
    ctx: &TxContext,
) {
    // We disallow registration after committee initiated in v1
    assert!(self.members.is_empty(), ECommitteeAlreadyInitiated);
    // Ensure pubkey is valid
    assert!(bridge_pubkey_bytes.length() == ECDSA_COMPRESSED_PUBKEY_LENGTH, EInvalidPubkeyLength);
    // sender must be the same sender that created the validator object, this is to prevent DDoS from non-validator actor.
    let sender = ctx.sender();
    let validators = system_state.active_validator_addresses();

    assert!(validators.contains(&sender), ESenderNotActiveValidator);
    // Sender is active validator, record the registration

    // In case validator need to update the info
    let registration = if (self.member_registrations.contains(&sender)) {
        let registration = &mut self.member_registrations[&sender];
        registration.http_rest_url = http_rest_url;
        registration.bridge_pubkey_bytes = bridge_pubkey_bytes;
        *registration
    } else {
        let registration = CommitteeMemberRegistration {
            sui_address: sender,
            bridge_pubkey_bytes,
            http_rest_url,
        };
        self.member_registrations.insert(sender, registration);
        registration
    };

    // check uniqueness of the bridge pubkey.
    // `try_create_next_committee` will abort if bridge_pubkey_bytes are not unique and
    // that will fail the end of epoch transaction (possibly "forever", well, we
    // need to deploy proper validator changes to stop end of epoch from failing).
    check_uniqueness_bridge_keys(self, bridge_pubkey_bytes);

    emit(registration)
}

// This method will try to create the next committee using the registration and system state,
// if the total stake fails to meet the minimum required percentage, it will skip the update.
// This is to ensure we don't fail the end of epoch transaction.
public(package) fun try_create_next_committee(
    self: &mut BridgeCommittee,
    active_validator_voting_power: VecMap<address, u64>,
    min_stake_participation_percentage: u64,
    ctx: &TxContext,
) {
    let mut i = 0;
    let mut new_members = vec_map::empty();
    let mut stake_participation_percentage = 0;

    while (i < self.member_registrations.size()) {
        // retrieve registration
        let (_, registration) = self.member_registrations.get_entry_by_idx(i);
        // Find validator stake amount from system state

        // Process registration if it's active validator
        let voting_power = active_validator_voting_power.try_get(&registration.sui_address);
        if (voting_power.is_some()) {
            let voting_power = voting_power.destroy_some();
            stake_participation_percentage = stake_participation_percentage + voting_power;

            let member = CommitteeMember {
                sui_address: registration.sui_address,
                bridge_pubkey_bytes: registration.bridge_pubkey_bytes,
                voting_power: (voting_power as u64),
                http_rest_url: registration.http_rest_url,
                blocklisted: false,
            };

            new_members.insert(registration.bridge_pubkey_bytes, member)
        };

        i = i + 1;
    };

    // Make sure the new committee represent enough stakes, percentage are accurate to 2DP
    if (stake_participation_percentage >= min_stake_participation_percentage) {
        // Clear registrations
        self.member_registrations = vec_map::empty();
        // Store new committee info
        self.members = new_members;
        self.last_committee_update_epoch = ctx.epoch();

        emit(CommitteeUpdateEvent {
            members: new_members,
            stake_participation_percentage,
        })
    }
}

// This function applys the blocklist to the committee members, we won't need to run this very often so this is not gas optimised.
// TODO: add tests for this function
public(package) fun execute_blocklist(self: &mut BridgeCommittee, blocklist: Blocklist) {
    let blocklisted = blocklist.blocklist_type() != 1;
    let eth_addresses = blocklist.blocklist_validator_addresses();
    let list_len = eth_addresses.length();
    let mut list_idx = 0;
    let mut member_idx = 0;
    let mut pub_keys = vector[];

    while (list_idx < list_len) {
        let target_address = &eth_addresses[list_idx];
        let mut found = false;

        while (member_idx < self.members.size()) {
            let (pub_key, member) = self.members.get_entry_by_idx_mut(member_idx);
            let eth_address = crypto::ecdsa_pub_key_to_eth_address(pub_key);

            if (*target_address == eth_address) {
                member.blocklisted = blocklisted;
                pub_keys.push_back(*pub_key);
                found = true;
                member_idx = 0;
                break
            };

            member_idx = member_idx + 1;
        };

        assert!(found, EValidatorBlocklistContainsUnknownKey);
        list_idx = list_idx + 1;
    };

    emit(BlocklistValidatorEvent {
        blocklisted,
        public_keys: pub_keys,
    })
}

public(package) fun committee_members(
    self: &BridgeCommittee,
): &VecMap<vector<u8>, CommitteeMember> {
    &self.members
}

public(package) fun update_node_url(
    self: &mut BridgeCommittee,
    new_url: vector<u8>,
    ctx: &TxContext,
) {
    let mut idx = 0;
    while (idx < self.members.size()) {
        let (_, member) = self.members.get_entry_by_idx_mut(idx);
        if (member.sui_address == ctx.sender()) {
            member.http_rest_url = new_url;
            emit(CommitteeMemberUrlUpdateEvent {
                member: member.bridge_pubkey_bytes,
                new_url,
            });
            return
        };
        idx = idx + 1;
    };
    abort ESenderIsNotInBridgeCommittee
}

// Assert if `bridge_pubkey_bytes` is duplicated in `member_registrations`.
// Dupicate keys would cause `try_create_next_committee` to fail and,
// in consequence, an end of epoch transaction to fail (safe mode run).
// This check will ensure the creation of the committee is correct.
fun check_uniqueness_bridge_keys(self: &BridgeCommittee, bridge_pubkey_bytes: vector<u8>) {
    let mut count = self.member_registrations.size();
    // bridge_pubkey_bytes must be found once and once only
    let mut bridge_key_found = false;
    while (count > 0) {
        count = count - 1;
        let (_, registration) = self.member_registrations.get_entry_by_idx(count);
        if (registration.bridge_pubkey_bytes == bridge_pubkey_bytes) {
            assert!(!bridge_key_found, EDuplicatePubkey);
            bridge_key_found = true; // bridge_pubkey_bytes found, we must not have another one
        }
    };
}

//////////////////////////////////////////////////////
// Test functions
//

#[test_only]
public(package) fun members(self: &BridgeCommittee): &VecMap<vector<u8>, CommitteeMember> {
    &self.members
}

#[test_only]
public(package) fun voting_power(member: &CommitteeMember): u64 {
    member.voting_power
}

#[test_only]
public(package) fun http_rest_url(member: &CommitteeMember): vector<u8> {
    member.http_rest_url
}

#[test_only]
public(package) fun member_registrations(
    self: &BridgeCommittee,
): &VecMap<address, CommitteeMemberRegistration> {
    &self.member_registrations
}

#[test_only]
public(package) fun blocklisted(member: &CommitteeMember): bool {
    member.blocklisted
}

#[test_only]
public(package) fun bridge_pubkey_bytes(registration: &CommitteeMemberRegistration): &vector<u8> {
    &registration.bridge_pubkey_bytes
}

#[test_only]
public(package) fun make_bridge_committee(
    members: VecMap<vector<u8>, CommitteeMember>,
    member_registrations: VecMap<address, CommitteeMemberRegistration>,
    last_committee_update_epoch: u64,
): BridgeCommittee {
    BridgeCommittee {
        members,
        member_registrations,
        last_committee_update_epoch,
    }
}

#[test_only]
public(package) fun make_committee_member(
    sui_address: address,
    bridge_pubkey_bytes: vector<u8>,
    voting_power: u64,
    http_rest_url: vector<u8>,
    blocklisted: bool,
): CommitteeMember {
    CommitteeMember {
        sui_address,
        bridge_pubkey_bytes,
        voting_power,
        http_rest_url,
        blocklisted,
    }
}
