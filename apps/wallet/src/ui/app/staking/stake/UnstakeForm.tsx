// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useEffect, useRef } from 'react';

import { Content } from '_app/shared/bottom-menu-layout';
import { Card } from '_app/shared/card';
import { Text } from '_app/shared/text';
import Alert from '_components/alert';
import NumberInput from '_components/number-input';
import { useFormatCoin } from '_hooks';
import { DEFAULT_GAS_BUDGET_FOR_STAKE } from '_redux/slices/sui-objects/Coin';

import type { FormValues } from './StakingCard';

export type StakeFromProps = {
    submitError: string | null;
    coinBalance: bigint;
    coinType: string;
    stakingReward?: number;
    onClearSubmitError: () => void;
};

export function UnStakeForm({
    submitError,
    coinBalance,
    coinType,
    onClearSubmitError,
    stakingReward,
}: StakeFromProps) {
    const { setFieldValue, setTouched } = useFormikContext<FormValues>();

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;

    const [gasBudgetEstimation, symbol] = useFormatCoin(
        DEFAULT_GAS_BUDGET_FOR_STAKE,
        SUI_TYPE_ARG
    );

    const [rewards, rewardSymbal] = useFormatCoin(stakingReward, SUI_TYPE_ARG);

    const [tokenBalance] = useFormatCoin(coinBalance, coinType);

    useEffect(() => {
        onClearRef.current();
        setFieldValue('amount', tokenBalance);
        setTouched({ amount: true });
    }, [setFieldValue, setTouched, tokenBalance]);

    return (
        <Form
            className="flex flex-1 flex-col flex-nowrap"
            autoComplete="off"
            noValidate={true}
        >
            <Content>
                <Field
                    component={NumberInput}
                    allowNegative={false}
                    name="amount"
                    hidden
                    className="w-full hidden"
                    decimals
                    disabled
                />
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
                                <Text
                                    variant="heading4"
                                    weight="semibold"
                                    color="steel-darker"
                                >
                                    {tokenBalance}
                                </Text>
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
                                {gasBudgetEstimation} {symbol}
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
                                {rewards} {rewardSymbal}
                            </Text>
                        </div>
                        <div className="w-2/3">
                            <Text
                                variant="p2"
                                weight="medium"
                                color="steel-darker"
                            >
                                Distributed at end of current Epoch
                            </Text>
                        </div>
                    </div>
                </Card>
                <ErrorMessage name="amount" component="div">
                    {(msg) => (
                        <div className="mt-2 flex flex-col flex-nowrap">
                            <Alert mode="warning" className="text-body">
                                {msg}
                            </Alert>
                        </div>
                    )}
                </ErrorMessage>

                {submitError ? (
                    <div className="mt-2 flex flex-col flex-nowrap">
                        <Alert mode="warning">
                            <strong>Unstake failed</strong>

                            <div>
                                <small>{submitError}</small>
                            </div>
                        </Alert>
                    </div>
                ) : null}
            </Content>
        </Form>
    );
}
