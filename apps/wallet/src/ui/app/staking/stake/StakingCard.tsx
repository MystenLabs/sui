// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionDigest, SUI_TYPE_ARG } from '@mysten/sui.js';
<<<<<<< HEAD
import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import StakeForm from './StakeForm';
import { ValidateDetailFormCard } from './ValidatorDetailCard';
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
=======
import BigNumber from 'bignumber.js';
import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate } from 'react-router-dom';

import StakeForm from './StakeForm';
import { createValidationSchema } from './validation';
import Loading from '_components/loading';
>>>>>>> 18c1164e1 (update)
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
<<<<<<< HEAD
import { Text } from '_src/ui/app/shared/text';
=======
>>>>>>> 18c1164e1 (update)

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

const initialValues = {
    amount: '',
};

export type FormValues = typeof initialValues;

<<<<<<< HEAD
function StakingCard() {
    const coinType = GAS_TYPE_ARG;

=======
export function StakingCard() {
    const coinType = GAS_TYPE_ARG;
>>>>>>> 18c1164e1 (update)
    const balances = useAppSelector(accountItemizedBalancesSelector);
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const coinBalance = useMemo(
        () => (coinType && aggregateBalances[coinType]) || BigInt(0),
        [coinType, aggregateBalances]
    );
<<<<<<< HEAD
    const [searchParams] = useSearchParams();
    const validatorAddress = searchParams.get('address');
    const isUnstake = searchParams.get('unstake') === 'true';
=======
>>>>>>> 18c1164e1 (update)
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

    const dispatch = useAppDispatch();
    const navigate = useNavigate();
<<<<<<< HEAD

=======
>>>>>>> 18c1164e1 (update)
    const onHandleSubmit = useCallback(
        async (
            { amount }: FormValues,
            { resetForm }: FormikHelpers<FormValues>
        ) => {
<<<<<<< HEAD
            if (coinType === null || validatorAddress === null) {
=======
            if (coinType === null) {
>>>>>>> 18c1164e1 (update)
                return;
            }
            setSendError(null);
            try {
<<<<<<< HEAD
                const bigIntAmount = parseAmount(amount, coinDecimals);
                // TODO: add unstake functionality on the support roles out
                if (isUnstake) return;
=======
                const bigIntAmount = BigInt(
                    new BigNumber(amount)
                        .shiftedBy(coinDecimals)
                        .integerValue()
                        .toString()
                );

>>>>>>> 18c1164e1 (update)
                const response = await dispatch(
                    stakeTokens({
                        amount: bigIntAmount,
                        tokenTypeArg: coinType,
<<<<<<< HEAD
                        validatorAddress: validatorAddress,
=======
>>>>>>> 18c1164e1 (update)
                    })
                ).unwrap();
                const txDigest = getTransactionDigest(response);
                resetForm();
<<<<<<< HEAD
                navigate(
                    `/receipt?${new URLSearchParams({
                        txdigest: txDigest,
                    }).toString()}`
                );
=======
                navigate(`/tx/${encodeURIComponent(txDigest)}`);
>>>>>>> 18c1164e1 (update)
            } catch (e) {
                setSendError((e as SerializedError).message || null);
            }
        },
<<<<<<< HEAD
        [
            coinType,
            validatorAddress,
            coinDecimals,
            isUnstake,
            dispatch,
            navigate,
        ]
    );

=======
        [dispatch, navigate, coinType, coinDecimals]
    );
>>>>>>> 18c1164e1 (update)
    const handleOnClearSubmitError = useCallback(() => {
        setSendError(null);
    }, []);
    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );

<<<<<<< HEAD
    if (!coinType || !validatorAddress) {
=======
    if (!coinType) {
>>>>>>> 18c1164e1 (update)
        return <Navigate to="/" replace={true} />;
    }

    return (
<<<<<<< HEAD
        <div className="flex flex-col flex-nowrap flex-grow h-full w-full">
            <Loading
                loading={loadingBalance}
                className="flex justify-center w-full items-center "
            >
                <Formik
                    initialValues={initialValues}
                    validateOnMount={true}
                    validationSchema={validationSchema}
                    onSubmit={onHandleSubmit}
                >
                    {({ isSubmitting, isValid, submitForm }) => (
                        <BottomMenuLayout>
                            <Content>
                                <ValidateDetailFormCard
                                    validatorAddress={validatorAddress}
                                    unstake={isUnstake}
                                />
                                <div className="flex flex-col justify-between items-center mb-2 mt-6 w-full">
                                    <Text
                                        variant="caption"
                                        color="gray-85"
                                        weight="semibold"
                                    >
                                        {isUnstake
                                            ? 'Enter the amount of SUI to unstake'
                                            : 'Enter the amount of SUI to stake'}
                                    </Text>
                                </div>
                                <StakeForm
                                    submitError={sendError}
                                    coinBalance={coinBalance}
                                    coinType={coinType}
                                    unstake={isUnstake}
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
                                    disabled={!isValid || isSubmitting}
                                >
                                    {isSubmitting ? (
                                        <LoadingIndicator />
                                    ) : isUnstake ? (
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
=======
        <>
            <h3>Stake {coinSymbol}</h3>
            <Loading loading={loadingBalance}>
                <Formik
                    initialValues={initialValues}
                    validateOnMount={false}
                    validationSchema={validationSchema}
                    onSubmit={onHandleSubmit}
                >
                    <StakeForm
                        submitError={sendError}
                        coinBalance={coinBalance.toString()}
                        coinType={coinType}
                        onClearSubmitError={handleOnClearSubmitError}
                    />
                </Formik>
            </Loading>
        </>
    );
}
>>>>>>> 18c1164e1 (update)
