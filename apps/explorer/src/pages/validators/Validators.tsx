// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject } from '@mysten/sui.js';
import { lazy, Suspense, useState, useMemo } from 'react';

import Pagination from '../../components/pagination/Pagination';

import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { StakeColumn } from '~/components/top-validators-card/StakeColumn';
import { useGetObject } from '~/hooks/useGetObject';
import { DelegationAmount } from '~/pages/validator/DelegationAmount';
import {
    VALIDATORS_OBJECT_ID,
    type ValidatorsFields,
} from '~/pages/validator/ValidatorDataTypes';
import { Banner } from '~/ui/Banner';
import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { ImageIcon } from '~/ui/ImageIcon';
import { ValidatorLink } from '~/ui/InternalLink';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { Stats } from '~/ui/Stats';
import { TableCard } from '~/ui/TableCard';
import { TableHeader } from '~/ui/TableHeader';
import { Text } from '~/ui/Text';
import { getName } from '~/utils/getName';

const NUMBER_OF_VALIDATORS = 20;

const ValidatorMap = lazy(
    () => import('../../components/validator-map/ValidatorMap')
);

function ValidatorPageResult() {
    const { data, isLoading, isSuccess, isError } =
        useGetObject(VALIDATORS_OBJECT_ID);

    const [validatorsPageNumber, setValidatorsPageNumber] = useState(1);

    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorsFields)
            : null;

    const totalStake = validatorsData?.validators.fields.total_validator_stake;

    const validatorsAPYs = useMemo(() => {
        if (!validatorsData) return [];
        const validators = validatorsData.validators.fields.active_validators;

        return validators.map((av) => {
            const { sui_balance, starting_epoch, delegation_token_supply } =
                av.fields.delegation_staking_pool.fields;

            const num_epochs_participated =
                ~validatorsData.epoch - ~starting_epoch;

            const APY = Math.pow(
                1 +
                    (~sui_balance - ~delegation_token_supply.fields.value) /
                        Number(delegation_token_supply.fields.value),
                365 / num_epochs_participated - 1
            );

            return APY ? APY : 0;
        });
    }, [validatorsData]);

    const averageAPY = useMemo(() => {
        if (!validatorsAPYs.length) return 0;

        return (
            validatorsAPYs.reduce((acc, cur) => acc + cur, 0) /
            validatorsAPYs.length
        );
    }, [validatorsAPYs]);

    const validatorsTableData = useMemo(() => {
        if (!validatorsData) return null;

        const validators = validatorsData.validators.fields.active_validators;

        return {
            data: validators.map((validator, index) => {
                const validatorName = getName(
                    validator.fields.metadata.fields.name
                );

                const commissionRate = Number(validator.fields.commission_rate);

                return {
                    number: index + 1,
                    name: (
                        <div className="flex items-center gap-2.5">
                            <ImageIcon
                                src={null}
                                size="sm"
                                label={validatorName}
                                fallback={validatorName}
                                circle
                            />
                            <Text
                                variant="bodySmall/medium"
                                color="steel-darker"
                            >
                                {validatorName}
                            </Text>
                        </div>
                    ),
                    stake: (
                        <StakeColumn stake={validator.fields.stake_amount} />
                    ),

                    commission: (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {commissionRate > 0
                                ? `${validator.fields.commission_rate}%`
                                : '--'}
                        </Text>
                    ),
                    address: (
                        <ValidatorLink
                            address={
                                validator.fields.metadata.fields.sui_address
                            }
                            noTruncate
                        />
                    ),
                };
            }),
            columns: [
                {
                    headerLabel: '#',
                    accessorKey: 'number',
                },
                {
                    headerLabel: 'Name',
                    accessorKey: 'name',
                },
                {
                    headerLabel: 'Stake',
                    accessorKey: 'stake',
                },
                {
                    headerLabel: 'Address',
                    accessorKey: 'address',
                },
                {
                    headerLabel: 'Commission',
                    accessorKey: 'commission',
                },
            ],
        };
    }, [validatorsData]);

    if (isError || (!isLoading && !validatorsTableData?.data.length)) {
        return (
            <Banner variant="error" fullWidth>
                Validator data could not be loaded
            </Banner>
        );
    }

    return (
        <div>
            <Heading as="h1" variant="heading2/bold">
                Validators
            </Heading>

            <div className="mt-8 flex w-full flex-col gap-5 md:flex-row">
                <div className="basis-full md:basis-1/2">
                    <ErrorBoundary>
                        <Card spacing="lg">
                            <div className="flex max-w-full flex-col gap-8">
                                <Heading
                                    as="div"
                                    variant="heading4/semibold"
                                    color="steel-darker"
                                >
                                    Validators
                                </Heading>
                                <div className="grid-col grid grid-flow-row grid-rows-2 justify-between gap-2.5 md:grid-cols-2 md:flex-row md:gap-8">
                                    <Stats
                                        label="Participation"
                                        tooltip="Coming soon"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading2/semibold"
                                            color="steel-darker"
                                        >
                                            --
                                        </Heading>
                                    </Stats>
                                    <Stats label="Total Staked">
                                        <DelegationAmount
                                            amount={totalStake || 0n}
                                            isStats
                                        />
                                    </Stats>
                                    <Stats
                                        label="Last Epoch Reward"
                                        tooltip="Coming soon"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading2/semibold"
                                            color="steel-darker"
                                        >
                                            --
                                        </Heading>
                                    </Stats>
                                    <Stats
                                        label="AVG APY"
                                        tooltip="Average APY"
                                    >
                                        <Heading
                                            as="h3"
                                            variant="heading2/semibold"
                                            color="steel-darker"
                                        >
                                            {averageAPY > 0
                                                ? `${averageAPY}%`
                                                : '--'}
                                        </Heading>
                                    </Stats>
                                </div>
                            </div>
                        </Card>
                    </ErrorBoundary>
                </div>

                <div className="basis-full md:basis-1/2">
                    <ErrorBoundary>
                        <Suspense fallback={null}>
                            <ValidatorMap />
                        </Suspense>
                    </ErrorBoundary>
                </div>
            </div>
            <div className="mt-8">
                <ErrorBoundary>
                    <TableHeader>All Validators</TableHeader>
                    {isLoading && (
                        <PlaceholderTable
                            rowCount={NUMBER_OF_VALIDATORS}
                            rowHeight="13px"
                            colHeadings={['Name', 'Address', 'Stake']}
                            colWidths={['220px', '220px', '220px']}
                        />
                    )}

                    {isSuccess && validatorsTableData?.data && (
                        <>
                            <TableCard
                                data={validatorsTableData.data.filter(
                                    (_, index) =>
                                        index >=
                                            (validatorsPageNumber - 1) *
                                                NUMBER_OF_VALIDATORS &&
                                        index <
                                            validatorsPageNumber *
                                                NUMBER_OF_VALIDATORS
                                )}
                                columns={validatorsTableData.columns}
                            />

                            {validatorsTableData.data.length >
                                NUMBER_OF_VALIDATORS && (
                                <Pagination
                                    totalItems={validatorsTableData.data.length}
                                    itemsPerPage={NUMBER_OF_VALIDATORS}
                                    currentPage={validatorsPageNumber}
                                    onPagiChangeFn={setValidatorsPageNumber}
                                    stats={{
                                        stats_text: 'Total Validators',
                                        count: validatorsTableData.data.length,
                                    }}
                                />
                            )}
                        </>
                    )}
                </ErrorBoundary>
            </div>
        </div>
    );
}

export { ValidatorPageResult };
