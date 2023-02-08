// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG } from '@mysten/sui.js';
import { Field, Form, useFormikContext } from 'formik';
import { memo, useCallback, useEffect } from 'react';

import Loading from '../../components/loading';
import { useGasBudgetInMist } from '../../hooks/useGasBudgetInMist';
import { Card } from '_app/shared/card';
import { Text } from '_app/shared/text';
import NumberInput from '_components/number-input';
import { useFormatCoin } from '_hooks';
import {
    DEFAULT_GAS_BUDGET_FOR_PAY,
    DEFAULT_GAS_BUDGET_FOR_STAKE,
} from '_redux/slices/sui-objects/Coin';

import type { FormValues } from './StakingCard';

const HIDE_MAX = true;

export type StakeFromProps = {
    coinBalance: bigint;
    coinType: string;
    epoch?: string;
};

function StakeForm({ coinBalance, coinType, epoch }: StakeFromProps) {
    const { setFieldValue } = useFormikContext<FormValues>();

    const { gasBudget: gasBudgetInMist, isLoading } = useGasBudgetInMist(
        DEFAULT_GAS_BUDGET_FOR_PAY * 4 + DEFAULT_GAS_BUDGET_FOR_STAKE
    );
    const [gasBudgetEstimation] = useFormatCoin(gasBudgetInMist, SUI_TYPE_ARG);

    let totalAvailableBalance =
        coinBalance -
        BigInt(coinType === SUI_TYPE_ARG ? gasBudgetInMist || 0 : 0);
    if (totalAvailableBalance < 0) {
        totalAvailableBalance = 0n;
    }

    const [maxToken, symbol, queryResult] = useFormatCoin(
        totalAvailableBalance,
        coinType
    );

    const setMaxToken = useCallback(() => {
        if (!maxToken) return;
        setFieldValue('amount', maxToken);
    }, [maxToken, setFieldValue]);
    useEffect(() => {
        setFieldValue(
            'gasBudget',
            isLoading ? '' : (gasBudgetInMist || 0).toString()
        );
    }, [setFieldValue, gasBudgetInMist, isLoading]);

    return (
        <Form
            className="flex flex-1 flex-col flex-nowrap items-center"
            autoComplete="off"
        >
            <Loading loading={isLoading}>
                <div className="flex flex-col justify-between items-center mb-3 mt-3.5 w-full gap-1.5">
                    <Text variant="caption" color="gray-85" weight="semibold">
                        Enter the amount of SUI to stake
                    </Text>
                    <Text variant="bodySmall" color="steel" weight="medium">
                        Available - {maxToken} {symbol}
                    </Text>
                </div>
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
                            {!HIDE_MAX ? (
                                <button
                                    className="bg-white border border-solid border-gray-60 hover:border-steel-dark rounded-2xl h-6 w-11 flex justify-center items-center cursor-pointer text-steel-darker hover:text-steel-darker text-bodySmall font-medium disabled:opacity-50 disabled:cursor-auto"
                                    onClick={setMaxToken}
                                    disabled={queryResult.isLoading}
                                    type="button"
                                >
                                    Max
                                </button>
                            ) : null}
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
                            {epoch ? `Epoch #${+epoch + 2}` : '--'}
                        </Text>
                    </div>
                </Card>
            </Loading>
        </Form>
    );
}

export default memo(StakeForm);
