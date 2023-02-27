// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16 } from '@mysten/icons';
import { SUI_TYPE_ARG, Coin as CoinAPI, type CoinStruct } from '@mysten/sui.js';
import { Field, Form, useFormikContext, Formik } from 'formik';
import { useMemo, useEffect } from 'react';

import { createValidationSchemaStepOne } from './validation';
import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import { Text } from '_app/shared/text';
import { IconTooltip } from '_app/shared/tooltip';
import { AddressInput } from '_components/address-input';
import Loading from '_components/loading';
import { parseAmount } from '_helpers';
import {
    useCoinDecimals,
    useFormatCoin,
    useAppSelector,
    useGasBudgetEstimationUnits,
    useGetCoins,
} from '_hooks';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { useGasBudgetInMist } from '_src/ui/app/hooks/useGasBudgetInMist';
import { InputWithAction } from '_src/ui/app/shared/InputWithAction';

const initialValues = {
    to: '',
    amount: '',
    isPayAllSui: false,
    // for gas validation purposes
    // to revalidate when amount changes
    gasInputBudgetEst: null as number | null,
};

export type FormValues = typeof initialValues;

export type SubmitProps = {
    to: string;
    amount: string;
    isPayAllSui: boolean;
    coinIds: string[];
    gasBudget: number;
};

export type SendTokenFormProps = {
    coinType: string;
    onSubmit: (values: SubmitProps) => void;
    initialAmount: string;
    initialTo: string;
};

export function GasBudgetEstimationComp({
    coinDecimals,
    coins,
}: {
    coinDecimals: number;
    coins: CoinStruct[] | null;
}) {
    const { values, setFieldValue } = useFormikContext<FormValues>();

    const gasBudgetEstimationUnits = useGasBudgetEstimationUnits(
        coins!,
        BigInt(parseAmount(values?.amount, coinDecimals))
    );
    const { gasBudget: gasBudgetEstimation, isLoading } = useGasBudgetInMist(
        gasBudgetEstimationUnits
    );

    const [formattedGas, gasSymbol] = useFormatCoin(
        gasBudgetEstimation,
        GAS_TYPE_ARG
    );

    // update the gasInputBudgetEst value when the amount changes
    useEffect(() => {
        setFieldValue('gasInputBudgetEst', gasBudgetEstimation);
    }, [gasBudgetEstimation, setFieldValue, values.amount]);

    return (
        <Loading loading={isLoading}>
            <div className="px-2 mt-3 mb-5 flex w-full gap-2 justify-between">
                <div className="flex gap-1">
                    <Text variant="body" color="gray-80" weight="medium">
                        Estimated Gas Fees
                    </Text>
                    <div className="text-gray-60 h-4 items-end flex">
                        <IconTooltip tip="Estimated Gas Fees" placement="top" />
                    </div>
                </div>
                <Text variant="body" color="gray-90" weight="medium">
                    {formattedGas} {gasSymbol}
                </Text>
            </div>
        </Loading>
    );
}

// Set the initial gasEstimation from initial amount
// base on the input amount feild update the gasEstimation value
// Seperating the gasEstimation from the formik context to access the input amount value and update the gasEstimation value
export function SendTokenForm({
    coinType,
    onSubmit,
    initialAmount = '',
    initialTo = '',
}: SendTokenFormProps) {
    const accountAddress = useAppSelector(({ account }) => account.address);
    // Get all coins of the type
    const { data: coinsData, isLoading: coinsIsLoading } = useGetCoins(
        coinType,
        accountAddress!
    );

    // Get SUI balance
    const { data: suiCoinsData, isLoading: suiCoinsIsLoading } = useGetCoins(
        SUI_TYPE_ARG,
        accountAddress!
    );

    // filter out locked lockedUntilEpoch
    const coins =
        coinsData?.filter(({ lockedUntilEpoch }) => !lockedUntilEpoch) || null;

    const suiCoins = suiCoinsData?.filter(
        ({ lockedUntilEpoch }) => !lockedUntilEpoch
    );

    const coinBalance =
        coins?.reduce((acc, { balance }) => {
            return acc + BigInt(balance);
        }, 0n) || BigInt(0n);

    const gasAggregateBalance =
        suiCoins?.reduce((acc, { balance }) => {
            return acc + BigInt(balance);
        }, 0n) || BigInt(0n);

    const coinSymbol = (coinType && CoinAPI.getCoinSymbol(coinType)) || '';

    const [coinDecimals] = useCoinDecimals(coinType);
    const [gasDecimals] = useCoinDecimals(SUI_TYPE_ARG);

    const maxSuiSingleCoinBalance = useMemo(() => {
        const maxCoin = suiCoins?.reduce(
            (max, { balance }) => (max < balance ? balance : max),
            0
        );
        return BigInt(maxCoin || 0);
    }, [suiCoins]);

    const gasBudgetEstimationUnits = useGasBudgetEstimationUnits(
        coins!,
        BigInt(
            parseAmount(
                initialAmount !== '' ? initialAmount : '0',
                coinDecimals
            )
        )
    );

    const { gasBudget: gasBudgetEstimation, isLoading } = useGasBudgetInMist(
        gasBudgetEstimationUnits
    );

    const validationSchemaStepOne = useMemo(
        () =>
            createValidationSchemaStepOne(
                coinType || '',
                coinBalance,
                coinSymbol,
                gasAggregateBalance,
                coinDecimals,
                gasDecimals,
                null,
                maxSuiSingleCoinBalance
            ),
        [
            coinType,
            coinBalance,
            coinSymbol,
            coinDecimals,
            gasDecimals,
            gasAggregateBalance,
            maxSuiSingleCoinBalance,
        ]
    );

    const [maxToken, symbol, queryResult] = useFormatCoin(
        coinBalance,
        coinType
    );

    return (
        <Loading
            loading={
                suiCoinsIsLoading ||
                coinsIsLoading ||
                isLoading ||
                queryResult.isLoading
            }
        >
            <Formik
                initialValues={{
                    amount: initialAmount,
                    to: initialTo,
                    isPayAllSui:
                        initialAmount === maxToken && coinType === SUI_TYPE_ARG,
                    gasInputBudgetEst: gasBudgetEstimation || null,
                }}
                validationSchema={validationSchemaStepOne}
                enableReinitialize
                validateOnMount
                validateOnChange
                onSubmit={({
                    to,
                    amount,
                    isPayAllSui,
                    gasInputBudgetEst,
                }: FormValues) => {
                    if (!gasInputBudgetEst || !coins || !suiCoins) return;
                    const coinsIDs = coins
                        .sort((a, b) => a.balance - b.balance)
                        .map(({ coinObjectId }) => coinObjectId);
                    const suiCoinsIds = suiCoins.map(
                        ({ coinObjectId }) => coinObjectId
                    );
                    const data = {
                        to,
                        amount,
                        isPayAllSui,
                        coinIds: isPayAllSui ? suiCoinsIds : coinsIDs,
                        gasBudget: gasInputBudgetEst,
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
                }) => {
                    const newPaySuiAll =
                        values.amount === maxToken && coinType === SUI_TYPE_ARG;
                    if (values.isPayAllSui !== newPaySuiAll) {
                        setFieldValue('isPayAllSui', newPaySuiAll);
                    }

                    return (
                        <BottomMenuLayout>
                            <Content>
                                <Form autoComplete="off" noValidate>
                                    <div className="w-full flex flex-col flex-grow">
                                        <div className="px-2 mb-2.5">
                                            <Text
                                                variant="caption"
                                                color="steel-dark"
                                                weight="semibold"
                                            >
                                                Select SUI Amount to Send
                                            </Text>
                                        </div>

                                        <InputWithAction
                                            name="amount"
                                            placeholder="0.00"
                                            prefix={
                                                values.isPayAllSui ? '~ ' : ''
                                            }
                                            actionText="Max"
                                            suffix={` ${symbol}`}
                                            type="number"
                                            actionType="button"
                                            allowNegative={false}
                                            allowDecimals
                                            rounded="lg"
                                            dark
                                            onActionClicked={() =>
                                                setFieldValue(
                                                    'amount',
                                                    coinType === SUI_TYPE_ARG
                                                        ? maxToken
                                                        : coinBalance?.toString(),
                                                    true
                                                )
                                            }
                                            actionDisabled={
                                                parseAmount(
                                                    values?.amount,
                                                    coinDecimals
                                                ) === coinBalance ||
                                                queryResult.isLoading ||
                                                !maxToken ||
                                                !values.gasInputBudgetEst
                                            }
                                        />
                                    </div>
                                    <GasBudgetEstimationComp
                                        coinDecimals={coinDecimals}
                                        coins={coins}
                                    />

                                    <div className="w-full flex gap-2.5 flex-col mt-7.5">
                                        <div className="px-2 tracking-wider">
                                            <Text
                                                variant="caption"
                                                color="steel-dark"
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
                                    disabled={!isValid || isSubmitting}
                                    size="tall"
                                    text={'Review'}
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
