// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Base64DataBuffer,
    isSuiObject,
    isSuiMoveObject,
    type GetObjectDataResponse,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';

import { ReactComponent as ArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';
import { IS_STATIC_ENV } from '../../utils/envUtil';
import { truncate } from '../../utils/stringUtils';
import { mockState } from './mockData';

import { useRpc } from '~/hooks/useRpc';
import { Banner } from '~/ui/Banner';
import { Link } from '~/ui/Link';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';

const VALIDATORS_OBJECT_ID = '0x05';
const TRUNCATE_LENGTH = 16;
const NUMBER_OF_VALIDATORS = 10;

export type ValidatorMetadata = {
    type: '0x2::validator::ValidatorMetadata';
    fields: {
        name: string;
        net_address: string;
        next_epoch_stake: number;
        pubkey_bytes: string;
        sui_address: string;
    };
};

export type Validator = {
    type: '0x2::validator::Validator';
    fields: {
        delegation: bigint;
        delegation_count: number;
        metadata: ValidatorMetadata;
        pending_delegation: bigint;
        pending_delegation_withdraw: bigint;
        pending_delegator_count: number;
        pending_delegator_withdraw_count: number;
        pending_stake: {
            type: '0x1::option::Option<0x2::balance::Balance<0x2::sui::SUI>>';
            fields: any[keyof string];
        };
        pending_withdraw: bigint;
        stake_amount: bigint;
    };
};

export const STATE_DEFAULT: ValidatorState = {
    delegation_reward: 0,
    epoch: 0,
    id: { id: '', version: 0 },
    parameters: {
        type: '0x2::sui_system::SystemParameters',
        fields: {
            max_validator_candidate_count: 0,
            min_validator_stake: BigInt(0),
        },
    },
    storage_fund: 0,
    treasury_cap: {
        type: '',
        fields: {},
    },
    validators: {
        type: '0x2::validator_set::ValidatorSet',
        fields: {
            delegation_stake: BigInt(0),
            active_validators: [],
            next_epoch_validators: [],
            pending_removals: '',
            pending_validators: '',
            quorum_stake_threshold: BigInt(0),
            total_validator_stake: BigInt(0),
        },
    },
};

const textDecoder = new TextDecoder();

export type ObjFields = {
    type: string;
    fields: any[keyof string];
};

export type SystemParams = {
    type: '0x2::sui_system::SystemParameters';
    fields: {
        max_validator_candidate_count: number;
        min_validator_stake: bigint;
    };
};

export type ValidatorState = {
    delegation_reward: number;
    epoch: number;
    id: { id: string; version: number };
    parameters: SystemParams;
    storage_fund: number;
    treasury_cap: ObjFields;
    validators: {
        type: '0x2::validator_set::ValidatorSet';
        fields: {
            delegation_stake: bigint;
            active_validators: Validator[];
            next_epoch_validators: Validator[];
            pending_removals: string;
            pending_validators: string;
            quorum_stake_threshold: bigint;
            total_validator_stake: bigint;
        };
    };
};

export function sortValidatorsByStake(validators: Validator[]) {
    validators.sort((a: Validator, b: Validator): number => {
        if (a.fields.stake_amount < b.fields.stake_amount) return 1;
        if (a.fields.stake_amount > b.fields.stake_amount) return -1;
        return 0;
    });
}

function stakeColumn(validator: {
    stake: BigInt;
    stakePercent: number;
}): JSX.Element {
    return (
        <div className="flex gap-1.5 items-end">
            <Text variant="bodySmall" color="steel-darker">
                {validator.stake.toString()}
            </Text>
            <Text variant="caption" color="steel">
                {validator.stakePercent.toFixed(2)}%
            </Text>
        </div>
    );
}

export function processValidators(set: Validator[], totalStake: bigint) {
    return set
        .map((av) => {
            const rawName = av.fields.metadata.fields.name;
            const name = textDecoder.decode(
                new Base64DataBuffer(rawName).getData()
            );
            return {
                name: name,
                address: av.fields.metadata.fields.sui_address,
                pubkeyBytes: av.fields.metadata.fields.pubkey_bytes,
                stake: av.fields.stake_amount,
                stakePercent: getStakePercent(
                    av.fields.stake_amount,
                    totalStake
                ),
                delegation_count: av.fields.delegation_count || 0,
            };
        })
        .sort((a, b) => (a.name > b.name ? 1 : -1));
}

export const getStakePercent = (stake: bigint, total: bigint): number =>
    Number(BigInt(stake) * BigInt(100)) / Number(total);

const validatorsTable = (validatorsData: ValidatorState, limit?: number) => {
    const totalStake = validatorsData.validators.fields.total_validator_stake;
    sortValidatorsByStake(validatorsData.validators.fields.active_validators);
    const validators = processValidators(
        validatorsData.validators.fields.active_validators,
        totalStake
    );

    let cumulativeStakePercent = 0;
    const validatorsItmes = limit ? validators.splice(0, limit) : validators;

    return {
        data: validatorsItmes.map((validator) => {
            cumulativeStakePercent += validator.stakePercent;
            return {
                name: (
                    <Text variant="bodySmall" color="steel-dark">
                        {validator.name}
                    </Text>
                ),
                stake: stakeColumn(validator),
                delegation: (
                    <Text variant="bodySmall" color="steel-darker">
                        {validator.stake.toString()}
                    </Text>
                ),
                cumulativeStake: (
                    <Text variant="bodySmall" color="steel-darker">
                        {cumulativeStakePercent.toFixed(2)}%
                    </Text>
                ),
                address: (
                    <Link
                        variant="mono"
                        to={`/addresses/${encodeURIComponent(
                            validator.address
                        )}`}
                    >
                        {truncate(validator.address, TRUNCATE_LENGTH)}
                    </Link>
                ),
                pubkeyBytes: (
                    <Text variant="bodySmall" color="steel-dark">
                        {truncate(validator.pubkeyBytes, TRUNCATE_LENGTH)}
                    </Text>
                ),
            };
        }),
        columns: [
            {
                headerLabel: 'Name',
                accessorKey: 'name',
            },
            {
                headerLabel: 'Address',
                accessorKey: 'address',
            },
            {
                headerLabel: 'Stake',
                accessorKey: 'stake',
            },
            {
                headerLabel: 'Distribution',
                accessorKey: 'cumulativeStake',
            },
            {
                headerLabel: 'Pubkey Bytes',
                accessorKey: 'pubkeyBytes',
            },
        ],
    };
};

function TopValidatorsCardStatic({ limit }: { limit?: number }) {
    const { data, columns } = validatorsTable(mockState, limit);
    return <TableCard data={data} columns={columns} />;
}

function TopValidatorsCardAPI({ limit }: { limit?: number }) {
    const rpc = useRpc();

    const { data, isLoading, isSuccess, isError } = useQuery(
        ['validatorS'],
        async () => {
            const validatorData: GetObjectDataResponse = await rpc.getObject(
                VALIDATORS_OBJECT_ID
            );
            if (
                !(
                    isSuiObject(validatorData.details) &&
                    isSuiMoveObject(validatorData.details.data)
                )
            ) {
                throw new Error(
                    'sui system state information not shaped as expected'
                );
            }
            return validatorData.details.data.fields as ValidatorState;
        }
    );

    const tableData = useMemo(
        () => (data ? validatorsTable(data, limit) : null),
        [data, limit]
    );

    if (isError || (!isLoading && !tableData?.data.length)) {
        return (
            <Banner variant="error" fullWidth>
                Validator data could not be loaded
            </Banner>
        );
    }

    return (
        <>
            {isLoading && (
                <PlaceholderTable
                    rowCount={limit || NUMBER_OF_VALIDATORS}
                    rowHeight="13px"
                    colHeadings={[
                        'Name',
                        'Address',
                        'Stake',
                        'Distribution',
                        'Pubkey Bytes',
                    ]}
                    colWidths={['135px', '135px', '90px', '135px', '220px']}
                />
            )}

            {isSuccess && tableData && (
                <>
                    <TableCard
                        data={tableData.data}
                        columns={tableData.columns}
                    />
                    {limit && (
                        <div className="mt-3">
                            <Link to="/validators">
                                <div className="flex items-center gap-2">
                                    More Validators{' '}
                                    <ArrowRight fill="currentColor" />
                                </div>
                            </Link>
                        </div>
                    )}
                </>
            )}
        </>
    );
}

export function TopValidatorsCard({ limit }: { limit?: number }) {
    return IS_STATIC_ENV ? (
        <TopValidatorsCardStatic limit={limit} />
    ) : (
        <TopValidatorsCardAPI limit={limit} />
    );
}
