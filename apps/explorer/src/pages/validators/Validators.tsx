// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Validator, type SuiEventEnvelope } from '@mysten/sui.js';
import { lazy, Suspense, useMemo } from 'react';

import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { StakeColumn } from '~/components/top-validators-card/StakeColumn';
import { DelegationAmount } from '~/components/validator/DelegationAmount';
import { calculateAPY } from '~/components/validator/calculateAPY';
import { useGetSystemObject } from '~/hooks/useGetObject';
import { useGetValidatorsEvents } from '~/hooks/useGetValidatorsEvents';
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
import { getValidatorMoveEvent } from '~/utils/getValidatorMoveEvent';
import { roundFloat } from '~/utils/roundFloat';

const APY_DECIMALS = 3;

const NodeMap = lazy(() => import('../../components/node-map'));

export function validatorsTableData(
    validators: Validator[],
    epoch: number,
    validatorsEvents: SuiEventEnvelope[],
    minimumStake: number
) {
    return {
        data: validators.map((validator) => {
            const validatorName = validator.metadata.name;
            const totalStake = validator.staking_pool.sui_balance;
            const img = validator.metadata.image_url;

            const event = getValidatorMoveEvent(
                validatorsEvents,
                validator.metadata.sui_address
            );

            return {
                name: {
                    name: validatorName,
                    logo: validator.metadata.image_url,
                },
                stake: totalStake,
                apy: calculateAPY(validator, epoch),
                commission: +validator.commission_rate / 100,
                img: img,
                address: validator.metadata.sui_address,
                lastReward: event?.fields.stake_rewards || 0,
                atRisk: totalStake < minimumStake,
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
                header: 'APY',
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
                header: 'Commission',
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
                header: 'Last Epoch Rewards',
                accessorKey: 'lastReward',
                cell: (props: any) => {
                    const lastReward = props.getValue();
                    return lastReward > 0 ? (
                        <StakeColumn stake={lastReward} hideCoinSymbol />
                    ) : (
                        <Text variant="bodySmall/medium" color="steel-darker">
                            --
                        </Text>
                    );
                },
            },
            {
                header: 'Status',
                accessorKey: 'atRisk',
                cell: (props: any) => {
                    const atRisk = props.getValue();
                    return atRisk ? (
                        <Text color="issue" variant="bodySmall/medium">
                            At Risk
                        </Text>
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
    const { data, isLoading, isSuccess, isError } = useGetSystemObject();

    const numberOfValidators = useMemo(
        () => data?.validators.active_validators.length || null,
        [data]
    );

    const {
        data: validatorEvents,
        isLoading: validatorsEventsLoading,
        isError: validatorEventError,
    } = useGetValidatorsEvents({
        limit: numberOfValidators,
        order: 'descending',
    });

    const totalStaked = useMemo(() => {
        if (!data) return 0;
        const validators = data.validators.active_validators;

        return validators.reduce(
            (acc, cur) => acc + +cur.staking_pool.sui_balance,
            0
        );
    }, [data]);

    const averageAPY = useMemo(() => {
        if (!data) return 0;
        const validators = data.validators.active_validators;

        const validatorsApy = validators.map((av) =>
            calculateAPY(av, +data.epoch)
        );
        return roundFloat(
            validatorsApy.reduce((acc, cur) => acc + cur, 0) /
                validatorsApy.length,
            APY_DECIMALS
        );
    }, [data]);

    const lastEpochRewardOnAllValidators = useMemo(() => {
        if (!validatorEvents) return 0;
        let totalRewards = 0;

        validatorEvents.data.forEach(({ event }) => {
            if ('moveEvent' in event) {
                const { moveEvent } = event;
                totalRewards += +moveEvent.fields.stake_rewards;
            }
        });

        return totalRewards;
    }, [validatorEvents]);

    const validatorsTable = useMemo(() => {
        if (!data || !validatorEvents) return null;

        const validators = data.validators.active_validators;

        return validatorsTableData(
            validators,
            +data.epoch,
            validatorEvents.data,
            data.parameters.min_validator_stake
        );
    }, [validatorEvents, data]);

    const defaultSorting = [{ id: 'stake', desc: false }];

    if (isError || validatorEventError) {
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
                                    label="Last Epoch SUI Rewards"
                                    tooltip="The stake rewards collected during the last epoch."
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
                                    unavailable={averageAPY <= 0}
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
                            defaultSorting={defaultSorting}
                        />
                    )}
                </ErrorBoundary>
            </div>
        </div>
    );
}

export { ValidatorPageResult };
