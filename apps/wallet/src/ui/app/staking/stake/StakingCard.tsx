// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getTransactionDigest,
    normalizeSuiAddress,
    SUI_TYPE_ARG,
} from '@mysten/sui.js';
import { useQueryClient } from '@tanstack/react-query';
import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import { STATE_OBJECT } from '../usePendingDelegation';
import StakeForm from './StakeForm';
import { ValidatorFormDetail } from './ValidatorFormDetail';
import { createValidationSchema } from './validation';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { parseAmount } from '_helpers';
import {
    useAppSelector,
    useAppDispatch,
    useCoinDecimals,
    useIndividualCoinMaxBalance,
} from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountItemizedBalancesSelector,
} from '_redux/slices/account';
import { Coin, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { stakeTokens } from '_redux/slices/transactions';
import { Text } from '_src/ui/app/shared/text';

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

const initialValues = {
    amount: '',
};

export type FormValues = typeof initialValues;

function StakingCard() {
    const coinType = GAS_TYPE_ARG;

    const balances = useAppSelector(accountItemizedBalancesSelector);
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const coinBalance = useMemo(
        () => (coinType && aggregateBalances[coinType]) || BigInt(0),
        [coinType, aggregateBalances]
    );
    const [searchParams] = useSearchParams();
    const validatorAddress = searchParams.get('address');
    const unstake = searchParams.get('unstake') === 'true';
    const totalGasCoins = useMemo(
        () => balances[GAS_TYPE_ARG]?.length || 0,
        [balances]
    );
    const gasAggregateBalance = useMemo(
        () => aggregateBalances[GAS_TYPE_ARG] || BigInt(0),
        [aggregateBalances]
    );
    const coinSymbol = useMemo(
        () => (coinType && Coin.getCoinSymbol(coinType)) || '',
        [coinType]
    );
    const [sendError, setSendError] = useState<string | null>(null);
    const [coinDecimals] = useCoinDecimals(coinType);
    const [gasDecimals] = useCoinDecimals(GAS_TYPE_ARG);
    const maxSuiSingleCoinBalance = useIndividualCoinMaxBalance(SUI_TYPE_ARG);
    const validationSchema = useMemo(
        () =>
            createValidationSchema(
                coinType || '',
                coinBalance,
                coinSymbol,
                gasAggregateBalance,
                totalGasCoins,
                coinDecimals,
                gasDecimals,
                maxSuiSingleCoinBalance
            ),
        [
            coinType,
            coinBalance,
            coinSymbol,
            gasAggregateBalance,
            totalGasCoins,
            coinDecimals,
            gasDecimals,
            maxSuiSingleCoinBalance,
        ]
    );

    const queryClient = useQueryClient();
    const dispatch = useAppDispatch();
    const navigate = useNavigate();

    const onHandleSubmit = useCallback(
        async (
            { amount }: FormValues,
            { resetForm }: FormikHelpers<FormValues>
        ) => {
            if (coinType === null || validatorAddress === null) {
                return;
            }
            setSendError(null);
            try {
                const bigIntAmount = parseAmount(amount, coinDecimals);
                // TODO: add unstake functionality
                if (unstake) return;
                const response = await dispatch(
                    stakeTokens({
                        amount: bigIntAmount,
                        tokenTypeArg: coinType,
                        validatorAddress: validatorAddress,
                    })
                ).unwrap();
                const txDigest = getTransactionDigest(response);
                //  invalidate the react query for 0x5
                queryClient.invalidateQueries({
                    queryKey: ['object', normalizeSuiAddress(STATE_OBJECT)],
                });
                resetForm();

                navigate(
                    `/receipt?${new URLSearchParams({
                        txdigest: txDigest,
                    }).toString()}`
                );
            } catch (e) {
                setSendError((e as SerializedError).message || null);
            }
        },
        [
            coinType,
            validatorAddress,
            coinDecimals,
            unstake,
            dispatch,
            queryClient,
            navigate,
        ]
    );

    const handleOnClearSubmitError = useCallback(() => {
        setSendError(null);
    }, []);
    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );

    if (!coinType || !validatorAddress) {
        return <Navigate to="/" replace={true} />;
    }

    return (
        <div className="flex flex-col flex-nowrap flex-grow w-full">
            <Loading
                loading={loadingBalance}
                className="flex justify-center w-full h-full items-center "
            >
                <Formik
                    initialValues={initialValues}
                    validateOnMount
                    validationSchema={validationSchema}
                    onSubmit={onHandleSubmit}
                >
                    {({ isSubmitting, isValid, submitForm }) => (
                        <BottomMenuLayout>
                            <Content>
                                <ValidatorFormDetail
                                    validatorAddress={validatorAddress}
                                    unstake={unstake}
                                />
                                <div className="flex flex-col justify-between items-center mb-2 mt-6 w-full">
                                    <Text
                                        variant="caption"
                                        color="gray-85"
                                        weight="semibold"
                                    >
                                        {unstake
                                            ? 'Enter the amount of SUI to unstake'
                                            : 'Enter the amount of SUI to stake'}
                                    </Text>
                                </div>
                                <StakeForm
                                    submitError={sendError}
                                    coinBalance={coinBalance}
                                    coinType={coinType}
                                    unstake={unstake}
                                    onClearSubmitError={
                                        handleOnClearSubmitError
                                    }
                                />
                            </Content>
                            <Menu
                                stuckClass="staked-cta"
                                className="w-full px-0 pb-0 mx-0"
                            >
                                <Button
                                    size="large"
                                    mode="neutral"
                                    href="/stake"
                                    disabled={isSubmitting}
                                    className="!text-steel-darker w-1/2"
                                >
                                    <Icon
                                        icon={SuiIcons.ArrowLeft}
                                        className="text-body text-gray-65 font-normal"
                                    />
                                    Back
                                </Button>
                                <Button
                                    size="large"
                                    mode="primary"
                                    onClick={submitForm}
                                    className=" w-1/2"
                                    disabled={
                                        !isValid || isSubmitting || unstake
                                    }
                                >
                                    {isSubmitting ? (
                                        <LoadingIndicator className="border-white" />
                                    ) : unstake ? (
                                        'Unstake Now'
                                    ) : (
                                        'Stake Now'
                                    )}
                                </Button>
                            </Menu>
                        </BottomMenuLayout>
                    )}
                </Formik>
            </Loading>
        </div>
    );
}

export default StakingCard;
