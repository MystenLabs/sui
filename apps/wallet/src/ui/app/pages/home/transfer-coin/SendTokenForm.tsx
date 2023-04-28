// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCoinDecimals, useFormatCoin, CoinFormat } from '@mysten/core';
import { ArrowRight16 } from '@mysten/icons';
import { SUI_TYPE_ARG, Coin as CoinAPI, type CoinStruct } from '@mysten/sui.js';
import { Field, Form, useFormikContext, Formik } from 'formik';
import { useMemo, useEffect } from 'react';

import { createTokenTransferTransaction } from './utils/transaction';
import { createValidationSchemaStepOne } from './validation';
import { useActiveAddress } from '_app/hooks/useActiveAddress';
import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import { Text } from '_app/shared/text';
import { AddressInput } from '_components/address-input';
import Alert from '_components/alert';
import Loading from '_components/loading';
import { parseAmount } from '_helpers';
import { useTransactionGasBudget, useGetCoins } from '_hooks';
import { GAS_SYMBOL } from '_src/ui/app/redux/slices/sui-objects/Coin';
import { InputWithAction } from '_src/ui/app/shared/InputWithAction';

const initialValues = {
    to: '',
    amount: '',
    isPayAllSui: false,
    gasBudgetEst: '',
};

export type FormValues = typeof initialValues;

export type SubmitProps = {
    to: string;
    amount: string;
    isPayAllSui: boolean;
    coinIds: string[];
    coins: CoinStruct[];
    gasBudgetEst: string;
};

export type SendTokenFormProps = {
    coinType: string;
    onSubmit: (values: SubmitProps) => void;
    initialAmount: string;
    initialTo: string;
};

function GasBudgetEstimation({
    coinDecimals,
    coins,
}: {
    coinDecimals: number;
    coins: CoinStruct[];
}) {
    const activeAddress = useActiveAddress();
    const { values, setFieldValue } = useFormikContext<FormValues>();

    const transaction = useMemo(() => {
        if (!values.amount || !values.to || !coins) return null;

        return createTokenTransferTransaction({
            to: values.to,
            amount: values.amount,
            coinType: SUI_TYPE_ARG,
            coinDecimals,
            isPayAllSui: values.isPayAllSui,
            coins,
        });
    }, [coinDecimals, coins, values.amount, values.isPayAllSui, values.to]);

    const { data: gasBudget } = useTransactionGasBudget(
        activeAddress,
        transaction
    );

    // gasBudgetEstimation should change when the amount above changes
    useEffect(() => {
        setFieldValue('gasBudgetEst', gasBudget, true);
    }, [gasBudget, setFieldValue, values.amount]);

    return (
        <div className="px-2 my-2 flex w-full gap-2 justify-between">
            <div className="flex gap-1">
                <Text variant="body" color="gray-80" weight="medium">
                    Estimated Gas Fees
                </Text>
            </div>
            <Text variant="body" color="gray-90" weight="medium">
                {gasBudget ? gasBudget + ' ' + GAS_SYMBOL : '--'}
            </Text>
        </div>
    );
}

// Set the initial gasEstimation from initial amount
// base on the input amount field update the gasEstimation value
// Separating the gasEstimation from the formik context to access the input amount value and update the gasEstimation value
export function SendTokenForm({
    coinType,
    onSubmit,
    initialAmount = '',
    initialTo = '',
}: SendTokenFormProps) {
    const activeAddress = useActiveAddress();
    // Get all coins of the type
    const { data: coinsData, isLoading: coinsIsLoading } = useGetCoins(
        coinType,
        activeAddress!
    );

    const { data: suiCoinsData, isLoading: suiCoinsIsLoading } = useGetCoins(
        SUI_TYPE_ARG,
        activeAddress!
    );

    const suiCoins = suiCoinsData;
    const coins = coinsData;
    const coinBalance = CoinAPI.totalBalance(coins || []);
    const suiBalance = CoinAPI.totalBalance(suiCoinsData || []);

    const coinSymbol = (coinType && CoinAPI.getCoinSymbol(coinType)) || '';
    const [coinDecimals, coinDecimalsQueryResult] = useCoinDecimals(coinType);

    const validationSchemaStepOne = useMemo(
        () =>
            createValidationSchemaStepOne(
                coinBalance,
                coinSymbol,
                coinDecimals
            ),
        [coinBalance, coinSymbol, coinDecimals]
    );

    const [tokenBalance, symbol, queryResult] = useFormatCoin(
        coinBalance,
        coinType,
        CoinFormat.FULL
    );

    // remove the comma from the token balance
    const formattedTokenBalance = tokenBalance.replace(/,/g, '');
    const initAmountBig = parseAmount(initialAmount, coinDecimals);

    return (
        <Loading
            loading={
                queryResult.isLoading ||
                coinDecimalsQueryResult.isLoading ||
                suiCoinsIsLoading ||
                coinsIsLoading
            }
        >
            <Formik
                initialValues={{
                    amount: initialAmount,
                    to: initialTo,
                    isPayAllSui:
                        !!initAmountBig &&
                        initAmountBig === coinBalance &&
                        coinType === SUI_TYPE_ARG,
                    gasBudgetEst: '',
                }}
                validationSchema={validationSchemaStepOne}
                enableReinitialize
                validateOnMount
                validateOnChange
                onSubmit={({
                    to,
                    amount,
                    isPayAllSui,
                    gasBudgetEst,
                }: FormValues) => {
                    if (!coins || !suiCoins) return;
                    const coinsIDs = [...coins]
                        .sort((a, b) => Number(b.balance) - Number(a.balance))
                        .map(({ coinObjectId }) => coinObjectId);

                    const data = {
                        to,
                        amount,
                        isPayAllSui,
                        coins,
                        coinIds: coinsIDs,
                        gasBudgetEst,
                    };
                    onSubmit(data);
                }}
            >
                {({
                    isValid,
                    isSubmitting,
                    setFieldValue,
                    values,
                    submitForm,
                    validateField,
                }) => {
                    const newPaySuiAll =
                        parseAmount(values.amount, coinDecimals) ===
                            coinBalance && coinType === SUI_TYPE_ARG;
                    if (values.isPayAllSui !== newPaySuiAll) {
                        setFieldValue('isPayAllSui', newPaySuiAll);
                    }

                    const hasEnoughBalance =
                        values.isPayAllSui ||
                        suiBalance >
                            parseAmount(values.gasBudgetEst, coinDecimals) +
                                parseAmount(
                                    coinType === SUI_TYPE_ARG
                                        ? values.amount
                                        : '0',
                                    coinDecimals
                                );

                    return (
                        <BottomMenuLayout>
                            <Content>
                                <Form autoComplete="off" noValidate>
                                    <div className="w-full flex flex-col flex-grow">
                                        <div className="px-2 mb-2.5">
                                            <Text
                                                variant="caption"
                                                color="steel"
                                                weight="semibold"
                                            >
                                                Select Coin Amount to Send
                                            </Text>
                                        </div>

                                        <InputWithAction
                                            type="numberInput"
                                            name="amount"
                                            placeholder="0.00"
                                            prefix={
                                                values.isPayAllSui ? '~ ' : ''
                                            }
                                            actionText="Max"
                                            suffix={` ${symbol}`}
                                            actionType="button"
                                            allowNegative={false}
                                            decimals
                                            rounded="lg"
                                            dark
                                            onActionClicked={async () => {
                                                // using await to make sure the value is set before the validation
                                                await setFieldValue(
                                                    'amount',
                                                    formattedTokenBalance
                                                );
                                                validateField('amount');
                                            }}
                                            actionDisabled={
                                                parseAmount(
                                                    values?.amount,
                                                    coinDecimals
                                                ) === coinBalance ||
                                                queryResult.isLoading ||
                                                !coinBalance
                                            }
                                        />
                                    </div>
                                    {!hasEnoughBalance && isValid ? (
                                        <div className="mt-3">
                                            <Alert>
                                                Insufficient SUI to cover
                                                transaction
                                            </Alert>
                                        </div>
                                    ) : null}

                                    {coins ? (
                                        <GasBudgetEstimation
                                            coinDecimals={coinDecimals}
                                            coins={coins}
                                        />
                                    ) : null}

                                    <div className="w-full flex gap-2.5 flex-col mt-7.5">
                                        <div className="px-2 tracking-wider">
                                            <Text
                                                variant="caption"
                                                color="steel"
                                                weight="semibold"
                                            >
                                                Enter Recipient Address
                                            </Text>
                                        </div>
                                        <div className="w-full flex relative items-center flex-col">
                                            <Field
                                                component={AddressInput}
                                                name="to"
                                                placeholder="Enter Address"
                                            />
                                        </div>
                                    </div>
                                </Form>
                            </Content>
                            <Menu
                                stuckClass="sendCoin-cta"
                                className="w-full px-0 pb-0 mx-0 gap-2.5"
                            >
                                <Button
                                    type="submit"
                                    onClick={submitForm}
                                    variant="primary"
                                    loading={isSubmitting}
                                    disabled={
                                        !isValid ||
                                        isSubmitting ||
                                        !hasEnoughBalance ||
                                        values.gasBudgetEst === ''
                                    }
                                    size="tall"
                                    text="Review"
                                    after={<ArrowRight16 />}
                                />
                            </Menu>
                        </BottomMenuLayout>
                    );
                }}
            </Formik>
        </Loading>
    );
}
