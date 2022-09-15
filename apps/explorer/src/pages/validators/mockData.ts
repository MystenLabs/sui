// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer } from '@mysten/sui.js';

import { type Validator, type ValidatorState } from './Validators';

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
        suiAddress: '0x693447224cf96c7b1f8de90d15198ceea22439e5',
    },
    {
        name: 'Blockdaemon',
        stake: BigInt(8_220_000),
        suiAddress: '0x4497a41750e240f7a3352215c78b1e8ce9d605c1',
    },
    {
        name: 'Kraken',
        stake: BigInt(4_650_000),
        suiAddress: '0xdb3d8f18e40e7fdcb4a3179e029044595e33cf76',
    },
    {
        name: 'Coinbase',
        stake: BigInt(4_550_000),
        suiAddress: '0xf2f70c204eed5c33a9bb3eb4c0e3048edbbc3ac3',
    },
    {
        name: 'a16z',
        stake: BigInt(2_860_000),
        suiAddress: '0xf446d537680e4601f8fdc922ab897cc78f3706d7',
    },
    {
        name: 'Figment',
        stake: BigInt(2_840_000),
        suiAddress: '0x1c83e3c2fe69dd15f60633cf072c3840adef504b',
    },
    {
        name: 'Another One',
        stake: BigInt(2_730_000),
        suiAddress: '0x62afdd1fb17dbeb5325a0f3f6d073ba05bf1b958',
    },
    {
        name: 'Someone Else',
        stake: BigInt(2_730_000),
        suiAddress: '0xdd0b64b253fe55b71f9784afee2dfa2e8bbd1ab7',
    },
    {
        name: '4Pool',
        stake: BigInt(2_730_000),
        suiAddress: '0xd4394c1577ca125630f652428d071a7b7dd047ad',
    },
    {
        name: '3Pool',
        stake: BigInt(2_730_000),
        suiAddress: '0xb0ecf49920c6a46104e94d810a9e81db17a6e866',
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
