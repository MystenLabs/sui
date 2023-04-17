// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormatCoin } from '@mysten/core';
import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { Form } from 'formik';
import { useMemo } from 'react';

import { useTransactionGasBudget, useActiveAddress } from '../../hooks';
import { GAS_SYMBOL } from '../../redux/slices/sui-objects/Coin';
import { Heading } from '../../shared/heading';
import { useGetTimeBeforeEpochNumber } from '../useGetTimeBeforeEpochNumber';
import { createUnstakeTransaction } from './utils/transaction';
import { Card } from '_app/shared/card';
import { Text } from '_app/shared/text';
import { CountDownTimer } from '_src/ui/app/shared/countdown-timer';

export type StakeFromProps = {
    stakedSuiId: string;
    coinBalance: bigint;
    coinType: string;
    stakingReward?: string;
    epoch: number;
};

export function UnStakeForm({
    stakedSuiId,
    coinBalance,
    coinType,
    stakingReward,
    epoch,
}: StakeFromProps) {
    const [rewards, rewardSymbol] = useFormatCoin(stakingReward, SUI_TYPE_ARG);
    const [totalSui] = useFormatCoin(
        BigInt(stakingReward || 0) + coinBalance,
        SUI_TYPE_ARG
    );
    const [tokenBalance] = useFormatCoin(coinBalance, coinType);

    const transaction = useMemo(
        () => createUnstakeTransaction(stakedSuiId),
        [stakedSuiId]
    );
    const activeAddress = useActiveAddress();
    const { data: gasBudget } = useTransactionGasBudget(
        activeAddress,
        transaction
    );

    const { data: currentEpochEndTime } = useGetTimeBeforeEpochNumber(
        epoch + 1 || 0
    );

    return (
        <Form
            className="flex flex-1 flex-col flex-nowrap"
            autoComplete="off"
            noValidate
        >
            <Card
                titleDivider
                header={
                    <div className="px-4 py-3 w-full flex bg-white justify-between">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Current Epoch Ends
                        </Text>
                        <div className="flex gap-0.5 ml-auto">
                            {currentEpochEndTime > 0 ? (
                                <CountDownTimer
                                    timestamp={currentEpochEndTime}
                                    variant="body"
                                    color="steel-dark"
                                    weight="medium"
                                    endLabel="--"
                                />
                            ) : (
                                <Text
                                    variant="body"
                                    weight="medium"
                                    color="steel-dark"
                                >
                                    Epoch #{epoch}
                                </Text>
                            )}
                        </div>
                    </div>
                }
                footer={
                    <div className="flex gap-0.5 justify-between w-full">
                        <Text
                            variant="pBodySmall"
                            weight="medium"
                            color="steel-darker"
                        >
                            Total unstaked SUI
                        </Text>
                        <div className="flex gap-0.5 ml-auto">
                            <Heading
                                variant="heading4"
                                weight="semibold"
                                color="steel-darker"
                                leading="none"
                            >
                                {totalSui}
                            </Heading>
                            <Text
                                variant="bodySmall"
                                weight="medium"
                                color="steel-dark"
                            >
                                {GAS_SYMBOL}
                            </Text>
                        </div>
                    </div>
                }
            >
                <div className="pb-3.75 flex flex-col  w-full gap-2">
                    <div className="flex gap-0.5 justify-between w-full">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Your Stake
                        </Text>
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            {tokenBalance} {GAS_SYMBOL}
                        </Text>
                    </div>
                    <div className="flex gap-0.5 justify-between w-full">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Staking Rewards Earned
                        </Text>
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            {rewards} {rewardSymbol}
                        </Text>
                    </div>
                </div>
            </Card>
            <div className="mt-4">
                <Card variant="gray">
                    <div className=" w-full flex justify-between">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Gas Fees
                        </Text>

                        <Text variant="body" weight="medium" color="steel-dark">
                            {gasBudget || '-'} {GAS_SYMBOL}
                        </Text>
                    </div>
                </Card>
            </div>
        </Form>
    );
}
