// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    roundFloat,
    useGetRollingAverageApys,
    type ApyByValidator,
    useGetValidatorsEvents,
    formatPercentageDisplay,
    useGetSystemState,
} from '@mysten/core';
import { type SuiEvent, type SuiValidatorSummary } from '@mysten/sui.js';
import { lazy, Suspense, useMemo } from 'react';

import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { StakeColumn } from '~/components/top-validators-card/StakeColumn';
import { DelegationAmount } from '~/components/validator/DelegationAmount';
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
import { Tooltip } from '~/ui/Tooltip';
import { getValidatorMoveEvent } from '~/utils/getValidatorMoveEvent';
import { VALIDATOR_LOW_STAKE_GRACE_PERIOD } from '~/utils/validatorConstants';

const NodeMap = lazy(() => import('../../components/node-map'));

export function validatorsTableData(
    validators: SuiValidatorSummary[],
    atRiskValidators: [string, string][],
    validatorEvents: SuiEvent[],
    rollingAverageApys: ApyByValidator | null
) {
    return {
        data: [...validators]
            .sort(() => 0.5 - Math.random())
            .map((validator) => {
                const validatorName = validator.name;
                const totalStake = validator.stakingPoolSuiBalance;
                const img = validator.imageUrl;

                const event = getValidatorMoveEvent(
                    validatorEvents,
                    validator.suiAddress
                );

                const atRiskValidator = atRiskValidators.find(
                    ([address]) => address === validator.suiAddress
                );
                const isAtRisk = !!atRiskValidator;
                const lastReward = event?.pool_staking_reward ?? null;

                return {
                    name: {
                        name: validatorName,
                        logo: validator.imageUrl,
                    },
                    stake: totalStake,
                    // show the rolling average apy even if its zero, otherwise show -- for no data
                    apy: rollingAverageApys?.[validator.suiAddress] ?? null,
                    nextEpochGasPrice: validator.nextEpochGasPrice,
                    commission: Number(validator.commissionRate) / 100,
                    img: img,
                    address: validator.suiAddress,
                    lastReward: lastReward ? Number(lastReward) : null,
                    votingPower: Number(validator.votingPower) / 100,
                    atRisk: isAtRisk
                        ? VALIDATOR_LOW_STAKE_GRACE_PERIOD -
                          Number(atRiskValidator[1])
                        : null,
                };
            }),
        columns: [
            {
                header: '#',
                accessorKey: 'number',
                cell: (props: any) => (
                    <Text variant="bodySmall/medium" color="steel-dark">
                        {props.table
                            .getSortedRowModel()
                            .flatRows.indexOf(props.row) + 1}
                    </Text>
                ),
            },
            {
                header: 'Name',
                accessorKey: 'name',
                enableSorting: true,
                sortingFn: (a: any, b: any, colId: string) =>
                    a
                        .getValue(colId)
                        .name.localeCompare(b.getValue(colId).name, 'en', {
                            sensitivity: 'base',
                            numeric: true,
                        }),
                cell: (props: any) => {
                    const { name, logo } = props.getValue();
                    return (
                        <Link
                            to={`/validator/${encodeURIComponent(
                                props.row.original.address
                            )}`}
                        >
                            <div className="flex items-center gap-2.5">
                                <ImageIcon
                                    src={logo}
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
                            </div>
                        </Link>
                    );
                },
            },
            {
                header: 'Stake',
                accessorKey: 'stake',
                enableSorting: true,
                cell: (props: any) => <StakeColumn stake={props.getValue()} />,
            },
            {
                header: 'Proposed Next Epoch Gas Price',
                accessorKey: 'nextEpochGasPrice',
                enableSorting: true,
                cell: (props: any) => (
                    <StakeColumn stake={props.getValue()} inMIST />
                ),
            },
            {
                header: 'APY',
                accessorKey: 'apy',
                cell: (props: any) => {
                    const apy = props.getValue();
                    return (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {formatPercentageDisplay(apy)}
                        </Text>
                    );
                },
            },
            {
                header: 'Commission',
                accessorKey: 'commission',
                cell: (props: any) => {
                    const commissionRate = props.getValue();
                    return (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {commissionRate}%
                        </Text>
                    );
                },
            },
            {
                header: 'Last Epoch Rewards',
                accessorKey: 'lastReward',
                cell: (props: any) => {
                    const lastReward = props.getValue();
                    return lastReward >= 0 ? (
                        <StakeColumn stake={lastReward} />
                    ) : (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            --
                        </Text>
                    );
                },
            },
            {
                header: 'Voting Power',
                accessorKey: 'votingPower',
                cell: (props: any) => {
                    const votingPower = props.getValue();
                    return (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            {votingPower}%
                        </Text>
                    );
                },
            },
            {
                header: 'Status',
                accessorKey: 'atRisk',
                cell: (props: any) => {
                    const atRisk = props.getValue();
                    return atRisk !== null ? (
                        <Tooltip tip="Staked SUI is below the minimum SUI stake threshold to remain a validator.">
                            <div className="flex cursor-pointer flex-nowrap items-center">
                                <Text color="issue" variant="bodySmall/medium">
                                    At Risk
                                </Text>
                                &nbsp;
                                <Text
                                    uppercase
                                    variant="bodySmall/medium"
                                    color="steel-dark"
                                >
                                    {atRisk > 1
                                        ? `in ${atRisk} epochs`
                                        : 'next epoch'}
                                </Text>
                            </div>
                        </Tooltip>
                    ) : (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            Active
                        </Text>
                    );
                },
            },
        ],
    };
}

function ValidatorPageResult() {
    const { data, isLoading, isSuccess, isError } = useGetSystemState();

    const numberOfValidators = data?.activeValidators.length || 0;

    const {
        data: validatorEvents,
        isLoading: validatorsEventsLoading,
        isError: validatorEventError,
    } = useGetValidatorsEvents({
        limit: numberOfValidators,
        order: 'descending',
    });

    const { data: rollingAverageApys } =
        useGetRollingAverageApys(numberOfValidators);

    const totalStaked = useMemo(() => {
        if (!data) return 0;
        const validators = data.activeValidators;

        return validators.reduce(
            (acc, cur) => acc + Number(cur.stakingPoolSuiBalance),
            0
        );
    }, [data]);

    const averageAPY = useMemo(() => {
        if (
            !rollingAverageApys ||
            Object.keys(rollingAverageApys)?.length === 0
        )
            return null;

        // exclude validators with no apy
        const apys = Object.values(rollingAverageApys)?.filter((a) => a > 0);
        const averageAPY = apys?.reduce((acc, cur) => acc + cur, 0);
        // in case of no apy, return 0
        return apys.length > 0 ? roundFloat(averageAPY / apys.length) : 0;
    }, [rollingAverageApys]);

    const lastEpochRewardOnAllValidators = useMemo(() => {
        if (!validatorEvents) return null;
        let totalRewards = 0;

        validatorEvents.forEach(({ parsedJson }) => {
            totalRewards += Number(parsedJson!.pool_staking_reward);
        });

        return totalRewards;
    }, [validatorEvents]);

    const validatorsTable = useMemo(() => {
        if (!data || !validatorEvents) return null;
        return validatorsTableData(
            data.activeValidators,
            data.atRiskValidators,
            validatorEvents,
            rollingAverageApys
        );
    }, [data, validatorEvents, rollingAverageApys]);

    if (isError || validatorEventError) {
        return (
            <Banner variant="error" fullWidth>
                Validator data could not be loaded
            </Banner>
        );
    }

    return (
        <div>
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
                                    label="Last Epoch Rewards"
                                    tooltip="The stake rewards collected during the last epoch."
                                    unavailable={
                                        lastEpochRewardOnAllValidators === null
                                    }
                                >
                                    <DelegationAmount
                                        amount={
                                            typeof lastEpochRewardOnAllValidators ===
                                            'number'
                                                ? lastEpochRewardOnAllValidators
                                                : 0n
                                        }
                                        isStats
                                    />
                                </Stats>
                            </div>
                            <div className="flex flex-col gap-8">
                                <Stats
                                    label="Total SUI Staked"
                                    tooltip="The total SUI staked on the network by validators and delegators to validate the network and earn rewards."
                                    unavailable={totalStaked <= 0}
                                >
                                    <DelegationAmount
                                        amount={totalStaked || 0n}
                                        isStats
                                    />
                                </Stats>
                                <Stats
                                    label="AVG APY"
                                    tooltip="The global average of annualized percentage yield of all participating validators."
                                    unavailable={averageAPY === null}
                                >
                                    {averageAPY}%
                                </Stats>
                            </div>
                        </div>
                    </div>
                </Card>

                <ErrorBoundary>
                    <Suspense fallback={null}>
                        <NodeMap minHeight={230} />
                    </Suspense>
                </ErrorBoundary>
            </div>
            <div className="mt-8">
                <ErrorBoundary>
                    <TableHeader>All Validators</TableHeader>
                    {(isLoading || validatorsEventsLoading) && (
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
                        />
                    )}
                </ErrorBoundary>
            </div>
        </div>
    );
}

export { ValidatorPageResult };
