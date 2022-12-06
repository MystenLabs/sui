// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Base64DataBuffer,
    isSuiObject,
    isSuiMoveObject,
    type Validator,
    type SuiSystemState,
    type SuiMoveObject,
} from '@mysten/sui.js';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

import { ReactComponent as ArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';

import { useGetObject } from '~/hooks/useGetObject';
import { Banner } from '~/ui/Banner';
import { AddressLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';

const VALIDATORS_OBJECT_ID = '0x05';
const NUMBER_OF_VALIDATORS = 10;

const textDecoder = new TextDecoder();

function StakeColumn(prop: { stake: bigint | number; stakePercent: number }) {
    return (
        <div className="flex items-end gap-0.5">
            <Text variant="bodySmall" color="steel-darker">
                {prop.stake.toString()}
            </Text>
            <Text variant="captionSmall" color="steel-dark">
                {prop.stakePercent.toFixed(2)}%
            </Text>
        </div>
    );
}

export function processValidators(
    set: SuiMoveObject<Validator>[],
    totalStake: bigint
) {
    return set.map((av) => {
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
                BigInt(av.fields.stake_amount),
                totalStake
            ),
        };
    });
}

export const getStakePercent = (stake: bigint, total: bigint): number => {
    const bnStake = new BigNumber(stake.toString());
    const bnTotal = new BigNumber(total.toString());
    return bnStake.div(bnTotal).multipliedBy(100).toNumber();
};

const validatorsTable = (systemState: SuiSystemState, limit?: number) => {
    const totalStake = systemState.validators.fields.total_validator_stake;

    const validators = processValidators(
        systemState.validators.fields.active_validators,
        BigInt(totalStake)
    ).sort((a, b) => (a.name > b.name ? 1 : -1));

    const validatorsItems = limit ? validators.splice(0, limit) : validators;

    return {
        data: validatorsItems.map((validator) => {
            return {
                name: (
                    <Text
                        variant="bodySmall"
                        color="steel-darker"
                        weight="medium"
                    >
                        {validator.name}
                    </Text>
                ),
                stake: (
                    <StakeColumn
                        stake={validator.stake}
                        stakePercent={validator.stakePercent}
                    />
                ),
                delegation: (
                    <Text variant="bodySmall" color="steel-darker">
                        {validator.stake.toString()}
                    </Text>
                ),
                address: (
                    <AddressLink
                        address={validator.address}
                        noTruncate={!limit}
                    />
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
        ],
    };
};

export function TopValidatorsCard({ limit }: { limit?: number }) {
    const { data, isLoading, isSuccess, isError } =
        useGetObject(VALIDATORS_OBJECT_ID);

    const validatorData =
        data && isSuiObject(data.details) && isSuiMoveObject(data.details.data)
            ? (data.details.data.fields as SuiSystemState)
            : null;

    const tableData = useMemo(
        () => (validatorData ? validatorsTable(validatorData, limit) : null),
        [validatorData, limit]
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
                    colHeadings={['Name', 'Address', 'Stake']}
                    colWidths={['220px', '220px', '220px']}
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
