// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { ErrorMessage, Field, Form, useFormikContext } from 'formik';
import { useRef, memo, useCallback } from 'react';

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
    epoch: string;
    onClearSubmitError: () => void;
};

function StakeForm({
    submitError,
    coinBalance,
    coinType,
    onClearSubmitError,
    epoch,
}: StakeFromProps) {
    const { setFieldValue } = useFormikContext<FormValues>();

    const onClearRef = useRef(onClearSubmitError);
    onClearRef.current = onClearSubmitError;

    const [gasBudgetEstimation] = useFormatCoin(
        DEFAULT_GAS_BUDGET_FOR_STAKE,
        SUI_TYPE_ARG
    );

    const coinBalanceMinusGas =
        coinBalance -
        BigInt(coinType === SUI_TYPE_ARG ? DEFAULT_GAS_BUDGET_FOR_STAKE : 0);

    const [maxToken, symbol, queryResult] = useFormatCoin(
        coinBalanceMinusGas,
        coinType
    );

    const setMaxToken = useCallback(() => {
        if (!maxToken) return;
        setFieldValue('amount', maxToken);
    }, [maxToken, setFieldValue]);

    return (
        <Form className="flex flex-1 flex-col flex-nowrap" autoComplete="off">
            <Content>
                <Card
                    variant="gray"
                    titleDivider
                    header={
                        <div className="p-2.5 w-full flex bg-white">
                            <Field
                                component={NumberInput}
                                allowNegative={false}
                                name="amount"
                                className="w-full border-none text-hero-dark text-heading4 font-semibold bg-white placeholder:text-gray-70 placeholder:font-semibold"
                                decimals
                            />

                            <button
                                className="bg-white border border-solid border-gray-60 hover:border-steel-dark rounded-2xl h-6 w-11 flex justify-center items-center cursor-pointer text-steel-darker hover:text-steel-darker text-bodySmall font-medium disabled:opacity-50 disabled:cursor-auto"
                                onClick={setMaxToken}
                                disabled={queryResult.isLoading}
                                type="button"
                            >
                                Max
                            </button>
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
                    <div className="pb-3.75 flex justify-between w-full">
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Staking Rewards Start
                        </Text>
                        <Text
                            variant="body"
                            weight="medium"
                            color="steel-darker"
                        >
                            Epoch #{+epoch + 1}
                        </Text>
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
                            <strong>Stake failed</strong>
                            <small>{submitError}</small>
                        </Alert>
                    </div>
                ) : null}
            </Content>
        </Form>
    );
}

export default memo(StakeForm);
