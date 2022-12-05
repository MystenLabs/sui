// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '@mysten/sui.js';

import { type Validator, type ValidatorState } from './TopValidatorsCard';

const encoder = new TextEncoder();

type TestValidatorInfo = {
    name: string;
    stake: BigInt;
    suiAddress: string;
};

const validators: TestValidatorInfo[] = [
    {
        name: 'Jump Crypto',
        stake: BigInt(9_220_000),
        suiAddress: 'sui1zupu00ayxqddcu3vrthm2ppe9409r504fqn7cjwl9lpmsjufqjhss6yl72',
    },
    {
        name: 'Blockdaemon',
        stake: BigInt(8_220_000),
        suiAddress: 'sui1r8e5df4tf99jwuf6s0n8mkdauspfcq3yd3xd5twej7e2qlshwamqyt60u9',
    },
    {
        name: 'Kraken',
        stake: BigInt(4_650_000),
        suiAddress: 'sui1tqdprxn9wmfm2q44m3ruthjf0dm5u6x2cdm3n2py44a57ete07gsg5xll6',
    },
    {
        name: 'Coinbase',
        stake: BigInt(4_550_000),
        suiAddress: 'sui1w9zfmw8lgxx6ngq9gc2r05yxh8c0lthws0zz72fgzmvgs8gdu4cqsdwhs2',
    },
    {
        name: 'a16z',
        stake: BigInt(2_860_000),
        suiAddress: 'sui1sau0w2w6j38k2tqtx0t87w9uaackz4gq5qagletswavsnc3n59ksjtk7gf',
    },
    {
        name: 'Figment',
        stake: BigInt(2_840_000),
        suiAddress: 'sui1nm3vwhtt858whaa5w3gepanuhqprujaq8vdsksq8a0usyv3mjxjq9nz5fq',
    },
    {
        name: 'Another One',
        stake: BigInt(2_730_000),
        suiAddress: 'sui1hexrm8m3zre03hjl5t8psga34427ply4kz29dze62w8zrkjlt9esv4rnx2',
    },
    {
        name: 'Someone Else',
        stake: BigInt(2_730_000),
        suiAddress: 'sui1cn6rfe7l2ngxtuwy4z2kpcaktljyghwh3c7jzevxh5w223dzpgxqz7l4hf',
    },
    {
        name: '4Pool',
        stake: BigInt(2_730_000),
        suiAddress: 'sui1mne690jmzjda8jj34cmsd6kju5vlct88azu3z8q5l2jf7yk9f24sdu9738',
    },
    {
        name: '3Pool',
        stake: BigInt(2_730_000),
        suiAddress: 'sui1mne690jmzjda8jj34cmsd6kju5vlct88azu3z8q5l2jf7yk9f24sdu9738',
    },
];

const validatorsTotalStake: bigint = validators
    .map((v) => v.stake)
    .reduce(
        (prev, current, _i, _arr): BigInt =>
            BigInt(prev as bigint) + BigInt(current as bigint)
    ) as bigint;

export const mockState: ValidatorState = {
    delegation_reward: 0,
    epoch: 0,
    id: {
        id: '',
        version: 0,
    },
    parameters: {
        type: '0x2::sui_system::SystemParameters',
        fields: {
            max_validator_candidate_count: 100,
            min_validator_stake: BigInt(10),
        },
    },
    storage_fund: 0,
    treasury_cap: {
        type: '',
        fields: undefined,
    },
    validators: {
        type: '0x2::validator_set::ValidatorSet',
        fields: {
            delegation_stake: BigInt(100000000),
            active_validators: validators.map((v) => getFullValidatorData(v)),
            next_epoch_validators: [],
            pending_removals: '',
            pending_validators: '',
            quorum_stake_threshold:
                (validatorsTotalStake * BigInt(3)) / BigInt(4),
            total_validator_stake: validatorsTotalStake,
        },
    },
};

function getFullValidatorData(partial: TestValidatorInfo): Validator {
    const name64 = new Base64DataBuffer(
        encoder.encode(partial.name)
    ).toString();
    return {
        type: '0x2::validator::Validator',
        fields: {
            delegation: BigInt(0),
            delegation_count: Number((partial.stake as bigint) / BigInt(10000)),
            metadata: {
                type: '0x2::validator::ValidatorMetadata',
                fields: {
                    name: name64,
                    net_address: '',
                    next_epoch_stake: 0,
                    pubkey_bytes: '',
                    sui_address: partial.suiAddress,
                },
            },
            pending_delegation: BigInt(0),
            pending_delegation_withdraw: BigInt(0),
            pending_delegator_count: 0,
            pending_delegator_withdraw_count: 0,
            pending_stake: {
                type: '0x1::option::Option<0x2::balance::Balance<0x2::sui::SUI>>',
                fields: {},
            },
            pending_withdraw: BigInt(0),
            stake_amount: partial.stake as bigint,
        },
    };
}
