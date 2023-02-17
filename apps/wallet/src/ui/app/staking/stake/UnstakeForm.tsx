// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { Form, useFormikContext } from 'formik';
import { useEffect } from 'react';

import LoadingIndicator from '../../components/loading/LoadingIndicator';
import { useGasBudgetInMist } from '../../hooks/useGasBudgetInMist';
import { Heading } from '../../shared/heading';
import { Card } from '_app/shared/card';
import { Text } from '_app/shared/text';
import { useFormatCoin } from '_hooks';
import { DEFAULT_GAS_BUDGET_FOR_STAKE } from '_redux/slices/sui-objects/Coin';

import type { FormValues } from './StakingCard';

export type StakeFromProps = {
    coinBalance: bigint;
    coinType: string;
    stakingReward?: number;
};

export function UnStakeForm({
    coinBalance,
    coinType,
    stakingReward,
}: StakeFromProps) {
    const { setFieldValue } = useFormikContext<FormValues>();
    const { gasBudget, isLoading } = useGasBudgetInMist(
        DEFAULT_GAS_BUDGET_FOR_STAKE
    );
    const [gasBudgetFormatted, symbol] = useFormatCoin(gasBudget, SUI_TYPE_ARG);
    const [rewards, rewardSymbol] = useFormatCoin(stakingReward, SUI_TYPE_ARG);
    const [tokenBalance] = useFormatCoin(coinBalance, coinType);
    useEffect(() => {
        setFieldValue(
            'gasBudget',
            isLoading ? '' : (gasBudget || 0).toString(),
            true
        );
    }, [setFieldValue, gasBudget, isLoading]);

    return (
        <Form
            className="flex flex-1 flex-col flex-nowrap"
            autoComplete="off"
            noValidate
        >
            <Card
                variant="gray"
                titleDivider
                header={
                    <div className="px-4 py-3 w-full flex bg-white justify-between">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Your Stake
                        </Text>
                        <div className="flex gap-0.5 items-end">
                            <Heading
                                variant="heading4"
                                weight="semibold"
                                color="steel-darker"
                                leading="none"
                            >
                                {tokenBalance}
                            </Heading>
                            <Text
                                variant="bodySmall"
                                weight="medium"
                                color="steel-dark"
                            >
                                {symbol}
                            </Text>
                        </div>
                    </div>
                }
                footer={
                    <div className="py-px flex justify-between w-full">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Gas Fees
                        </Text>
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            {isLoading ? (
                                <LoadingIndicator />
                            ) : (
                                `${gasBudgetFormatted} ${symbol}`
                            )}
                        </Text>
                    </div>
                }
            >
                <div className="pb-3.75 flex flex-col  w-full gap-2">
                    <div className="flex gap-0.5 justify-between w-full">
                        <Text
                            variant="body"
                            weight="semibold"
                            color="steel-darker"
                        >
                            Staking Rewards
                        </Text>
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            {rewards} {rewardSymbol}
                        </Text>
                    </div>
                    <div className="w-2/3">
                        <Text variant="p2" weight="medium" color="steel-darker">
                            Distributed at end of current Epoch
                        </Text>
                    </div>
                </div>
            </Card>
        </Form>
    );
}
