// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo } from 'react';

import { DelegationAmount } from './DelegationAmount';

import type { Validator } from '~/pages/validator/ValidatorDataTypes';

import { Card } from '~/ui/Card';
import { Heading } from '~/ui/Heading';
import { Stats } from '~/ui/Stats';
import { getStakedPercent } from '~/utils/getStakedPercent';

type StatsCardProps = {
    validatorData: Validator;
    totalValidatorStake: bigint;
    epoch: number;
};

export function ValidatorStats({
    validatorData,
    epoch,
    totalValidatorStake,
}: StatsCardProps) {
    // TODO: add missing fields
    // const numberOfDelegators = 0;
    //  const selfStake = 0;
    //  const lastEpoch = 0;
    //  const totalRewards =  0;
    //  const networkStakingParticipation = 0;
    //  const votedLastRound =  0;
    //  const tallyingScore =  0;
    //  const lastNarwhalRound = 0;

    const validator = useMemo(() => {
        const { sui_balance, starting_epoch, delegation_token_supply } =
            validatorData.fields.delegation_staking_pool.fields;

        const num_epochs_participated = epoch - starting_epoch;

        const APY = Math.pow(
            1 +
                (sui_balance - delegation_token_supply.fields.value) /
                    delegation_token_supply.fields.value,
            365 / num_epochs_participated - 1
        );

        return {
            apy: APY ? APY : 0,
            delegatedStakePercentage: getStakedPercent(
                validatorData.fields.stake_amount,
                totalValidatorStake
            ),
            totalStake: validatorData.fields.stake_amount,
        };
    }, [validatorData, epoch, totalValidatorStake]);

    return (
        <div className="flex w-full flex-col gap-5 md:mt-8 md:flex-row">
            <div className="max-w-[480px] basis-full md:basis-2/5">
                <Card spacing="lg">
                    <div className="flex max-w-full flex-col flex-nowrap gap-8">
                        <Heading
                            as="div"
                            variant="heading4/semibold"
                            color="steel-darker"
                        >
                            SUI Staked on Validator
                        </Heading>
                        <div className="flex flex-col flex-nowrap gap-8 md:flex-row">
                            <Stats label="Staking APY" tooltip="Coming soon">
                                <Heading
                                    as="h3"
                                    variant="heading2/semibold"
                                    color="steel-darker"
                                >
                                    {validator.apy ? `${validator.apy}%` : '--'}
                                </Heading>
                            </Stats>
                            <Stats label="Total Staked" tooltip="Coming soon">
                                <DelegationAmount
                                    amount={validator.totalStake}
                                    isStats
                                />
                            </Stats>
                        </div>
                        <div className="flex flex-col flex-nowrap gap-8 md:flex-row">
                            <Stats label="Delegators" tooltip="Delegators">
                                <Heading
                                    as="h3"
                                    variant="heading3/semibold"
                                    color="steel-darker"
                                >
                                    --
                                </Heading>
                            </Stats>
                            <Stats
                                label="Delegated Staked"
                                tooltip="Coming soon"
                            >
                                <Heading
                                    as="h3"
                                    variant="heading3/semibold"
                                    color="steel-darker"
                                >
                                    {validator.delegatedStakePercentage}%
                                </Heading>
                            </Stats>
                            <Stats label="Self Staked" tooltip="Coming soon">
                                <Heading
                                    as="h3"
                                    variant="heading3/semibold"
                                    color="steel-darker"
                                >
                                    --
                                </Heading>
                            </Stats>
                        </div>
                    </div>
                </Card>
            </div>
            <div className="basis-full md:basis-1/4">
                <Card spacing="lg">
                    <div className="flex  max-w-full flex-col flex-nowrap gap-8">
                        <Heading
                            as="div"
                            variant="heading4/semibold"
                            color="steel-darker"
                        >
                            Validator Staking Rewards
                        </Heading>
                        <div className="flex flex-col flex-nowrap gap-8">
                            <Stats label="Last Epoch" tooltip="Coming soon">
                                <Heading
                                    as="h3"
                                    variant="heading3/semibold"
                                    color="steel-darker"
                                >
                                    --
                                </Heading>
                            </Stats>
                            <Stats label="Total Reward" tooltip="Coming soon">
                                <Heading
                                    as="h3"
                                    variant="heading3/semibold"
                                    color="steel-darker"
                                >
                                    --
                                </Heading>
                            </Stats>
                        </div>
                    </div>
                </Card>
            </div>
            <div className="max-w-[432px] basis-full md:basis-1/3">
                <Card spacing="lg">
                    <div className="flex  max-w-full flex-col flex-nowrap gap-8">
                        <Heading
                            as="div"
                            variant="heading4/semibold"
                            color="steel-darker"
                        >
                            Network Participation
                        </Heading>
                        <div className="flex flex-col flex-nowrap gap-8">
                            <div className="flex flex-col flex-nowrap gap-8 md:flex-row">
                                <Stats
                                    label="Staking Participation"
                                    tooltip="Coming soon"
                                >
                                    <Heading
                                        as="h3"
                                        variant="heading3/semibold"
                                        color="steel-darker"
                                    >
                                        --
                                    </Heading>
                                </Stats>

                                <Stats
                                    label="voted Last Round"
                                    tooltip="Coming soon"
                                >
                                    <Heading
                                        as="h3"
                                        variant="heading3/semibold"
                                        color="steel-darker"
                                    >
                                        --
                                    </Heading>
                                </Stats>
                            </div>
                            <div className="flex flex-col flex-nowrap gap-8 md:flex-row">
                                <Stats
                                    label="Tallying Score"
                                    tooltip="Coming soon"
                                >
                                    <Heading
                                        as="h3"
                                        variant="heading3/semibold"
                                        color="steel-darker"
                                    >
                                        --
                                    </Heading>
                                </Stats>

                                <Stats
                                    label="Last Narwhal Round"
                                    tooltip="Coming soon"
                                >
                                    <Heading
                                        as="h3"
                                        variant="heading3/semibold"
                                        color="steel-darker"
                                    >
                                        --
                                    </Heading>
                                </Stats>
                            </div>
                        </div>
                    </div>
                </Card>
            </div>
        </div>
    );
}
