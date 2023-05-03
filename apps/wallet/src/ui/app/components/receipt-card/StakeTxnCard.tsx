// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    formatPercentageDisplay,
    useGetValidatorsApy,
    useGetTimeBeforeEpochNumber,
} from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';

import { Card } from '../../shared/transaction-summary/Card';
import { ValidatorLogo } from '_app/staking/validators/ValidatorLogo';
import { TxnAmount } from '_components/receipt-card/TxnAmount';
import {
    NUM_OF_EPOCH_BEFORE_STAKING_REWARDS_REDEEMABLE,
    NUM_OF_EPOCH_BEFORE_STAKING_REWARDS_STARTS,
} from '_src/shared/constants';
import { CountDownTimer } from '_src/ui/app/shared/countdown-timer';
import { Text } from '_src/ui/app/shared/text';
import { IconTooltip } from '_src/ui/app/shared/tooltip';

import type { SuiEvent } from '@mysten/sui.js';

type StakeTxnCardProps = {
    event: SuiEvent;
};

// For Staked Transaction use moveEvent Field to get the validator address, delegation amount, epoch
export function StakeTxnCard({ event }: StakeTxnCardProps) {
    const validatorAddress = event.parsedJson?.validator_address;
    const stakedAmount = event.parsedJson?.amount;
    const stakedEpoch = Number(event.parsedJson?.epoch || 0);

    const { data: rollingAverageApys } = useGetValidatorsApy();

    const { apy, isApyApproxZero } = rollingAverageApys?.[validatorAddress] ?? {
        apy: null,
    };
    // Reward will be available after 2 epochs
    // TODO: Get epochStartTimestampMs/StartDate
    // for staking epoch + NUM_OF_EPOCH_BEFORE_STAKING_REWARDS_REDEEMABLE
    const startEarningRewardsEpoch =
        Number(stakedEpoch) + NUM_OF_EPOCH_BEFORE_STAKING_REWARDS_STARTS;

    const redeemableRewardsEpoch =
        Number(stakedEpoch) + NUM_OF_EPOCH_BEFORE_STAKING_REWARDS_REDEEMABLE;

    const { data: timeBeforeStakeRewardsStarts } = useGetTimeBeforeEpochNumber(
        startEarningRewardsEpoch
    );

    const { data: timeBeforeStakeRewardsRedeemable } =
        useGetTimeBeforeEpochNumber(redeemableRewardsEpoch);

    return (
        <Card>
            <div className="flex flex-col divide-y divide-solid divide-gray-40 divide-x-0">
                {validatorAddress && (
                    <div className="mb-3.5 w-full divide-y divide-gray-40 divide-solid">
                        <ValidatorLogo
                            validatorAddress={validatorAddress}
                            showAddress
                            iconSize="md"
                            size="body"
                            activeEpoch={event.parsedJson?.epoch}
                        />
                    </div>
                )}
                {stakedAmount && (
                    <TxnAmount
                        amount={stakedAmount}
                        coinType={SUI_TYPE_ARG}
                        label="Stake"
                    />
                )}
                <div className="flex flex-col">
                    <div className="flex justify-between w-full py-3.5">
                        <div className="flex gap-1 items-baseline justify-center text-steel">
                            <Text
                                variant="body"
                                weight="medium"
                                color="steel-darker"
                            >
                                APY
                            </Text>
                            <IconTooltip tip="This is the Annualized Percentage Yield of the a specific validator’s past operations. Note there is no guarantee this APY will be true in the future." />
                        </div>
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            {formatPercentageDisplay(
                                apy,
                                '--',
                                isApyApproxZero
                            )}
                        </Text>
                    </div>
                </div>
                <div className="flex flex-col">
                    <div className="flex justify-between w-full py-3.5">
                        <div className="flex gap-1 items-baseline text-steel">
                            <Text
                                variant="body"
                                weight="medium"
                                color="steel-darker"
                            >
                                {timeBeforeStakeRewardsStarts > 0
                                    ? 'Staking Rewards Start'
                                    : 'Staking Rewards Started'}
                            </Text>
                        </div>

                        {timeBeforeStakeRewardsStarts > 0 ? (
                            <CountDownTimer
                                timestamp={timeBeforeStakeRewardsStarts}
                                variant="body"
                                color="steel-darker"
                                weight="medium"
                                label="in"
                                endLabel="--"
                            />
                        ) : (
                            <Text
                                variant="body"
                                weight="medium"
                                color="steel-darker"
                            >
                                Epoch #{startEarningRewardsEpoch}
                            </Text>
                        )}
                    </div>
                    <div className="flex justify-between w-full">
                        <div className="flex gap-1 flex-1 items-baseline text-steel">
                            <Text
                                variant="pBody"
                                weight="medium"
                                color="steel-darker"
                            >
                                Staking Rewards Redeemable
                            </Text>
                        </div>
                        <div className="flex-1 flex justify-end gap-1 items-center">
                            {timeBeforeStakeRewardsRedeemable > 0 ? (
                                <CountDownTimer
                                    timestamp={timeBeforeStakeRewardsRedeemable}
                                    variant="body"
                                    color="steel-darker"
                                    weight="medium"
                                    label="in"
                                    endLabel="--"
                                />
                            ) : (
                                <Text
                                    variant="body"
                                    weight="medium"
                                    color="steel-darker"
                                >
                                    Epoch #{redeemableRewardsEpoch}
                                </Text>
                            )}
                        </div>
                    </div>
                </div>
            </div>
        </Card>
    );
}
