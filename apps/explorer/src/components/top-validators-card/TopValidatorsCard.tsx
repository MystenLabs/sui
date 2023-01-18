// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject, type ValidatorsFields, type ActiveValidator } from '@mysten/sui.js';
import { useMemo } from 'react';

import { ReactComponent as ArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';
import { StakeColumn } from './StakeColumn';

import { useGetObject } from '~/hooks/useGetObject';
import {
    VALIDATORS_OBJECT_ID,
} from '~/pages/validator/ValidatorDataTypes';
import { Banner } from '~/ui/Banner';
import { ImageIcon } from '~/ui/ImageIcon';
import { ValidatorLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';
import { getName } from '~/utils/getName';
import { getStakedPercent } from '~/utils/getStakedPercent';

const NUMBER_OF_VALIDATORS = 10;

export function processValidators(set: ActiveValidator[], totalStake: bigint) {
    return set.map((av) => {
        const rawName = av.fields.metadata.fields.name;
        return {
            name: getName(rawName),
            address: av.fields.metadata.fields.sui_address,
            stake: av.fields.stake_amount,
            stakePercent: getStakedPercent(av.fields.stake_amount, totalStake),
            logo: null,
        };
    });
}

const validatorsTable = (
    validatorsData: ValidatorsFields,
    limit?: number,
    showIcon?: boolean
) => {
    const totalStake = BigInt(
        validatorsData.validators.fields.total_validator_stake
    );

    const validators = processValidators(
        validatorsData.validators.fields.active_validators,
        totalStake
    ).sort((a, b) =>
        a.name.localeCompare(b.name, 'en', {
            sensitivity: 'base',
            numeric: true,
        })
    );

    const validatorsItems = limit ? validators.splice(0, limit) : validators;

    return {
        data: validatorsItems.map(({ name, stake, address, logo }) => {
            return {
                name: (
                    <div className="flex items-center gap-2.5">
                        {showIcon && (
                            <ImageIcon
                                src={logo}
                                size="sm"
                                fallback={name}
                                label={name}
                                circle
                            />
                        )}

                        <Link to={`/validator/${encodeURIComponent(address)}`}>
                            {name}
                        </Link>
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
                header: 'Name',
                accessorKey: 'name',
            },
            {
                header: 'Address',
                accessorKey: 'address',
            },
            {
                header: 'Stake',
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
            ? (data.details.data.fields as ValidatorsFields)
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
