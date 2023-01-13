// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject } from '@mysten/sui.js';
import { lazy, Suspense, useMemo } from 'react';

import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { StakeColumn } from '~/components/top-validators-card/StakeColumn';
import { DelegationAmount } from '~/components/validator/DelegationAmount';
import { calculateAPY } from '~/components/validator/calculateAPY';
import { useGetObject } from '~/hooks/useGetObject';
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

const ValidatorMap = lazy(() => import('../../components/node-map'));

function ValidatorPageResult() {
    const { data, isLoading, isSuccess, isError } =
        useGetObject(VALIDATORS_OBJECT_ID);

    const validatorsData =
        data &&
        is(data.details, SuiObject) &&
        data.details.data.dataType === 'moveObject'
            ? (data.details.data.fields as ValidatorsFields)
            : null;

    const totalStake = validatorsData?.validators.fields.total_validator_stake;

    const averageAPY = useMemo(() => {
        if (!validatorsData) return 0;
        const validators = validatorsData.validators.fields.active_validators;

        const validatorsApy = validators.map((av) =>
            calculateAPY(av, +validatorsData.epoch)
        );
        return (
            validatorsApy.reduce((acc, cur) => acc + cur, 0) /
            validatorsApy.length
        );
    }, [validatorsData]);

    const validatorsTableData = useMemo(() => {
        if (!validatorsData) return null;

        const validators = validatorsData.validators.fields.active_validators;

        return {
            data: validators.map((validator, index) => {
                const validatorName = getName(
                    validator.fields.metadata.fields.name
                );

                const commissionRate = +validator.fields.commission_rate;

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

            <div className="mt-8 grid gap-5 md:grid-cols-2">
                <Card spacing="lg">
                    <div className="flex w-full basis-full flex-col gap-8">
                        <Heading
                            as="div"
                            variant="heading4/semibold"
                            color="steel-darker"
                        >
                            Validators
                        </Heading>

                        <div className="flex flex-col gap-8 md:flex-row">
                            <div className="flex flex-col gap-8">
                                <Stats
                                    label="Participation"
                                    tooltip="Coming soon"
                                    unavailable
                                />

                                <Stats
                                    label="Last Epoch Reward"
                                    tooltip="Coming soon"
                                    unavailable
                                />
                            </div>
                            <div className="flex flex-col gap-8">
                                <Stats label="Total Staked">
                                    <DelegationAmount
                                        amount={totalStake || 0n}
                                        isStats
                                    />
                                </Stats>
                                <Stats
                                    label="AVG APY"
                                    tooltip="Average APY"
                                    unavailable={averageAPY <= 0}
                                >
                                    <Heading
                                        as="h3"
                                        variant="heading2/semibold"
                                        color="steel-darker"
                                    >
                                        {averageAPY}%
                                    </Heading>
                                </Stats>
                            </div>
                        </div>
                    </div>
                </Card>

                <ErrorBoundary>
                    <Suspense fallback={null}>
                        <ValidatorMap />
                    </Suspense>
                </ErrorBoundary>
            </div>
            <div className="mt-8">
                <ErrorBoundary>
                    <TableHeader>All Validators</TableHeader>
                    {isLoading && (
                        <PlaceholderTable
                            rowCount={20}
                            rowHeight="13px"
                            colHeadings={['Name', 'Address', 'Stake']}
                            colWidths={['220px', '220px', '220px']}
                        />
                    )}

                    {isSuccess && validatorsTableData?.data && (
                        <TableCard
                            data={validatorsTableData.data}
                            columns={validatorsTableData.columns}
                        />
                    )}
                </ErrorBoundary>
            </div>
        </div>
    );
}

export { ValidatorPageResult };
