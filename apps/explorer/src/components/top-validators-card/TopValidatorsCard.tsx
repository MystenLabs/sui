// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    is,
    SuiObject,
    type MoveSuiSystemObjectFields,
    type MoveActiveValidator,
} from '@mysten/sui.js';
import { useMemo } from 'react';

import { ReactComponent as ArrowRight } from '../../assets/SVGIcons/12px/ArrowRight.svg';
import { StakeColumn } from './StakeColumn';

import { useGetObject } from '~/hooks/useGetObject';
import { VALIDATORS_OBJECT_ID } from '~/pages/validator/ValidatorDataTypes';
import { Banner } from '~/ui/Banner';
import { ImageIcon } from '~/ui/ImageIcon';
import { ValidatorLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { TableCard } from '~/ui/TableCard';
import { Text } from '~/ui/Text';
import { getName } from '~/utils/getName';

const NUMBER_OF_VALIDATORS = 10;

export function processValidators(set: MoveActiveValidator[]) {
    return set.map((av) => {
        const rawName = av.fields.metadata.fields.name;
        const delegatedStake =
            +av.fields.delegation_staking_pool.fields.sui_balance;
        const selfStake = +av.fields.stake_amount;
        const totalValidatorStake = selfStake + delegatedStake;
        return {
            name: getName(rawName),
            address: av.fields.metadata.fields.sui_address,
            stake: totalValidatorStake,
            logo:
                typeof av.fields.metadata.fields.image_url === 'string'
                    ? av.fields.metadata.fields.image_url
                    : null,
        };
    });
}

const validatorsTable = (
    validatorsData: MoveSuiSystemObjectFields,
    limit?: number,
    showIcon?: boolean
) => {
    const validators = processValidators(
        validatorsData.validators.fields.active_validators
    ).sort((a, b) => (Math.random() > 0.5 ? -1 : 1));

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
            ? (data.details.data.fields as MoveSuiSystemObjectFields)
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
