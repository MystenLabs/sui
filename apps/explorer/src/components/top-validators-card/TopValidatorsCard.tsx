// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer, is, SuiObject, SUI_TYPE_ARG } from '@mysten/sui.js';
import BigNumber from 'bignumber.js';
import { useMemo } from 'react';

import { ReactComponent as ArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';

import { useFormatCoin } from '~/hooks/useFormatCoin';
import { useGetObject } from '~/hooks/useGetObject';
import {
    VALIDATORS_OBJECT_ID,
    type ValidatorState,
    type Validator,
} from '~/pages/validator/ValidatorDataTypes';
import { Banner } from '~/ui/Banner';
import { ImageIcon } from '~/ui/ImageIcon';
import { ValidatorLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';
import { getName } from '~/utils/getName';

const NUMBER_OF_VALIDATORS = 10;

function StakeColumn({ stake }: { stake: bigint }) {
    const [amount, symbol] = useFormatCoin(stake, SUI_TYPE_ARG);
    return (
        <div className="flex items-end gap-0.5">
            <Text variant="bodySmall/medium" color="steel-darker">
                {amount}
            </Text>
            <Text variant="captionSmall/medium" color="steel-dark">
                {symbol}
            </Text>
        </div>
    );
}

export function processValidators(set: Validator[], totalStake: bigint) {
    return set.map((av) => {
        const rawName = av.fields.metadata.fields.name;
        return {
            name: getName(rawName),
            address: av.fields.metadata.fields.sui_address,
            stake: av.fields.stake_amount,
            stakePercent: getStakePercent(av.fields.stake_amount, totalStake),
            delegation_count: av.fields.delegation_count || 0,
            logo: null,
        };
    });
}

export const getStakePercent = (stake: bigint, total: bigint): number => {
    const bnStake = new BigNumber(stake.toString());
    const bnTotal = new BigNumber(total.toString());
    return bnStake.div(bnTotal).multipliedBy(100).toNumber();
};

const validatorsTable = (
    validatorsData: ValidatorState,
    limit?: number,
    showIcon?: boolean
) => {
    const totalStake = validatorsData.validators.fields.total_validator_stake;

    const validators = processValidators(
        validatorsData.validators.fields.active_validators,
        totalStake
    ).sort((a, b) => (a.name > b.name ? 1 : -1));

    const validatorsItems = limit ? validators.splice(0, limit) : validators;

    return {
        data: validatorsItems.map(({ name, stake, address, logo }) => {
            return {
                name: (
                    <div className="flex items-center gap-2.5 capitalize">
                        {showIcon && (
                            <ImageIcon src={logo} size="sm" alt={name} />
                        )}
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {name}
                        </Text>
                    </div>
                ),
                stake: <StakeColumn stake={stake} />,
                delegation: (
                    <Text variant="bodySmall/medium" color="steel-darker">
                        {stake.toString()}
                    </Text>
                ),
                address: (
                    <ValidatorLink address={address} noTruncate={!limit} />
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

type TopValidatorsCardProps = {
    limit?: number;
    showIcon?: boolean;
};

export function TopValidatorsCard({ limit, showIcon }: TopValidatorsCardProps) {
    const { data, isLoading, isSuccess, isError } =
        useGetObject(VALIDATORS_OBJECT_ID);

    const validatorData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorState)
            : null;

    const tableData = useMemo(
        () =>
            validatorData
                ? validatorsTable(validatorData, limit, showIcon)
                : null,
        [validatorData, limit, showIcon]
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
                                    More Validators
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
