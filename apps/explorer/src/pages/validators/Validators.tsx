// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObject, type ActiveValidator } from '@mysten/sui.js';
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
import { Link } from '~/ui/Link';
import { PlaceholderTable } from '~/ui/PlaceholderTable';
import { Stats } from '~/ui/Stats';
import { TableCard } from '~/ui/TableCard';
import { TableHeader } from '~/ui/TableHeader';
import { Text } from '~/ui/Text';
import { getName } from '~/utils/getName';

const ValidatorMap = lazy(() => import('../../components/node-map'));

function validatorsTableDatas(validators: ActiveValidator[], epoch: number) {
    return {
        data: validators.map((validator, index) => {
            const validatorName = getName(
                validator.fields.metadata.fields.name
            );
            return {
                number: index + 1,
                name: validatorName,
                stake: validator.fields.stake_amount,
                apy: calculateAPY(validator, epoch),
                commission: +validator.fields.commission_rate,
                address: validator.fields.metadata.fields.sui_address,
                lastEpoch:
                    validator.fields.delegation_staking_pool.fields
                        .rewards_pool,
            };
        }),
        columns: [
            {
                headerLabel: '#',
                accessorKey: 'number',
                cell: (props: any) => (
                    <Text variant="bodySmall/medium" color="steel-dark">
                        {props.getValue()}
                    </Text>
                ),
            },
            {
                headerLabel: 'Name',
                accessorKey: 'name',
                enableSorting: true,
                cell: (props: any) => {
                    const name = props.getValue();
                    return (
                        <Link
                            to={`/validator/${encodeURIComponent(
                                props.row.original.address
                            )}`}
                        >
                            <div className="flex items-center gap-2.5">
                                <ImageIcon
                                    src={null}
                                    size="sm"
                                    label={name}
                                    fallback={name}
                                    circle
                                />
                                <Text
                                    variant="bodySmall/medium"
                                    color="steel-darker"
                                >
                                    {name}
                                </Text>
                            </div>{' '}
                        </Link>
                    );
                },
            },
            {
                headerLabel: 'Stake',
                accessorKey: 'stake',
                enableSorting: true,
                cell: (props: any) => <StakeColumn stake={props.getValue()} />,
            },
            {
                headerLabel: 'APY',
                accessorKey: 'apy',
                cell: (props: any) => {
                    const apy = props.getValue();
                    return (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {apy > 0 ? `${apy}%` : '--'}
                        </Text>
                    );
                },
            },
            {
                headerLabel: 'Commission',
                accessorKey: 'commission',
                cell: (props: any) => {
                    const commissionRate = props.getValue();
                    return (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {commissionRate > 0 ? `${commissionRate}%` : '--'}
                        </Text>
                    );
                },
            },
            {
                headerLabel: 'Last Epoch Rewards',
                accessorKey: 'lastEpoch',
                cell: (props: any) => {
                    const lastEpochReward = props.getValue();
                    return lastEpochReward > 0 ? (
                        <StakeColumn stake={lastEpochReward} hideCoinSymbol />
                    ) : (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            --
                        </Text>
                    );
                },
            },
        ],
    };
}

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

    const lastEpochRewardOnAllValidators = useMemo(() => {
        if (!validatorsData) return 0;
        const validators = validatorsData.validators.fields.active_validators;

        return validators.reduce(
            (acc, cur) =>
                acc + +cur.fields.delegation_staking_pool.fields.rewards_pool,
            0
        );
    }, [validatorsData]);

    const validatorsTable = useMemo(() => {
        if (!validatorsData) return null;

        const validators = validatorsData.validators.fields.active_validators;

        return validatorsTableDatas(validators, +validatorsData.epoch);
    }, [validatorsData]);

    const defaultSorting = [{ id: 'stake', desc: true }];

    if (isError || (!isLoading && !validatorsTable?.data.length)) {
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
                                    unavailable={
                                        lastEpochRewardOnAllValidators <= 0
                                    }
                                >
                                    <DelegationAmount
                                        amount={
                                            lastEpochRewardOnAllValidators || 0n
                                        }
                                        isStats
                                    />
                                </Stats>
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

                    {isSuccess && validatorsTable?.data && (
                        <TableCard
                            data={validatorsTable.data}
                            columns={validatorsTable.columns}
                            sortTable
                            defaultSorting={defaultSorting}
                        />
                    )}
                </ErrorBoundary>
            </div>
        </div>
    );
}

export { ValidatorPageResult };
