// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16, ArrowLeft16 } from '@mysten/icons';
import {
    Coin as CoinAPI,
    getTransactionDigest,
    SUI_TYPE_ARG,
} from '@mysten/sui.js';
import { Formik, type FormikHelpers } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import { StepOne } from './TransferCoinForm/StepOne';
import { StepTwo } from './TransferCoinForm/StepTwo';
import { createValidationSchemaStepOne } from './validation';
import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import ActiveCoinsCard from '_components/active-coins-card';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { parseAmount } from '_helpers';
import {
    useAppSelector,
    useAppDispatch,
    useCoinDecimals,
    useIndividualCoinMaxBalance,
} from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountCoinsSelector,
} from '_redux/slices/account';
import { Coin } from '_redux/slices/sui-objects/Coin';
import { sendTokens } from '_redux/slices/transactions';
import { trackEvent } from '_src/shared/plausible';
import { useGasBudgetInMist } from '_src/ui/app/hooks/useGasBudgetInMist';

import type { SerializedError } from '@reduxjs/toolkit';

const initialValues = {
    to: '',
    amount: '',
    isPayAllSui: false,
};

const DEFAULT_FORM_STEP = 1;

export type FormValues = typeof initialValues;

// TODO: show out of sync when sui objects locally might be outdated
function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const [showModal, setShowModal] = useState(true);

    const coinType = searchParams.get('type');
    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const coinBalance = useMemo(
        () => (coinType && aggregateBalances[coinType]) || BigInt(0),
        [coinType, aggregateBalances]
    );
    const gasAggregateBalance = useMemo(
        () => aggregateBalances[SUI_TYPE_ARG] || BigInt(0),
        [aggregateBalances]
    );
    const coinSymbol = useMemo(
        () => (coinType && CoinAPI.getCoinSymbol(coinType)) || '',
        [coinType]
    );
    const allCoins = useAppSelector(accountCoinsSelector);
    const allCoinsOfTransferType = useMemo(
        () => allCoins.filter((aCoin) => aCoin.type === coinType),
        [allCoins, coinType]
    );
    const [sendError, setSendError] = useState<string | null>(null);
    const [currentStep, setCurrentStep] = useState<number>(DEFAULT_FORM_STEP);
    const [formData, setFormData] = useState<FormValues>(initialValues);

    const [coinDecimals] = useCoinDecimals(coinType);
    const [gasDecimals] = useCoinDecimals(SUI_TYPE_ARG);
    const [amountToSend, setAmountToSend] = useState(BigInt(0));
    const maxSuiSingleCoinBalance = useIndividualCoinMaxBalance(SUI_TYPE_ARG);
    const gasBudgetEstimationUnits = useMemo(
        () => Coin.computeGasBudgetForPay(allCoinsOfTransferType, amountToSend),
        [allCoinsOfTransferType, amountToSend]
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
                gasBudgetEstimation || 0,
                maxSuiSingleCoinBalance
            ),
        [
            coinType,
            coinBalance,
            coinSymbol,
            coinDecimals,
            gasDecimals,
            gasAggregateBalance,
            gasBudgetEstimation,
            maxSuiSingleCoinBalance,
        ]
    );

    const dispatch = useAppDispatch();
    const navigate = useNavigate();

    const closeSendToken = useCallback(() => {
        navigate('/');
    }, [navigate]);

    const onHandleSubmit = useCallback(
        async ({ to, amount, isPayAllSui }: FormValues) => {
            if (coinType === null || !gasBudgetEstimationUnits) {
                return;
            }

            setSendError(null);
            trackEvent('TransferCoins', {
                props: { coinType },
            });
            try {
                const bigIntAmount = parseAmount(amount, coinDecimals);
                //Todo:(Jibz) move to react-query
                const response = await dispatch(
                    sendTokens({
                        amount: bigIntAmount,
                        recipientAddress: to,
                        tokenTypeArg: coinType,
                        gasBudget: gasBudgetEstimationUnits,
                    })
                ).unwrap();

                const txDigest = getTransactionDigest(response);
                const receiptUrl = `/receipt?txdigest=${encodeURIComponent(
                    txDigest
                )}&from=transactions`;

                navigate(receiptUrl);
            } catch (e) {
                setSendError((e as SerializedError).message || null);
            }
        },
        [dispatch, navigate, coinType, coinDecimals, gasBudgetEstimationUnits]
    );

    const handleOnClearSubmitError = useCallback(() => {
        setSendError(null);
    }, []);
    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );

    const handleNextStep = useCallback(
        (
            formData: FormValues,
            { setSubmitting }: FormikHelpers<FormValues>
        ) => {
            setCurrentStep((prev) => prev + 1);
            setFormData(formData);
            setSubmitting(false);
        },
        []
    );

    if (!coinType) {
        return <Navigate to="/" replace={true} />;
    }

    const StepOneForm = (
        <div className="flex flex-col w-full mt-2.5">
            <div className="mb-7">
                <ActiveCoinsCard activeCoinType={coinType} />
            </div>

            <Formik
                initialValues={formData}
                validationSchema={validationSchemaStepOne}
                onSubmit={handleNextStep}
                enableReinitialize={true}
            >
                <StepOne
                    submitError={sendError}
                    coinType={coinType}
                    balance={coinBalance}
                    gasCostEstimation={gasBudgetEstimation || null}
                    onClearSubmitError={handleOnClearSubmitError}
                    onAmountChanged={(anAmount) => setAmountToSend(anAmount)}
                />
            </Formik>
        </div>
    );

    const StepTwoPreview = (
        <BottomMenuLayout>
            <Content>
                <div className="flex flex-col w-full mt-2.5">
                    <StepTwo
                        coinType={coinType}
                        amount={formData.amount}
                        to={formData.to}
                        gasCostEstimation={gasBudgetEstimation || null}
                        onClearSubmitError={handleOnClearSubmitError}
                    />
                </div>
            </Content>
            <Menu
                stuckClass="sendCoin-cta"
                className="w-full px-0 pb-0 mx-0 gap-2.5"
            >
                <Button
                    type="button"
                    variant="secondary"
                    onClick={() => setCurrentStep(1)}
                    text={'Back'}
                    before={<ArrowLeft16 />}
                />

                <Button
                    type="button"
                    variant="primary"
                    onClick={() => onHandleSubmit}
                    size="tall"
                    text={'Review'}
                    after={<ArrowRight16 />}
                />
            </Menu>
        </BottomMenuLayout>
    );

    const steps = [StepOneForm, StepTwoPreview];

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={currentStep === 1 ? 'Send Coins' : 'Review & Send'}
            closeOverlay={closeSendToken}
            closeIcon={SuiIcons.Close}
        >
            <Loading loading={loadingBalance || isLoading}>
                {steps[currentStep - 1]}
            </Loading>
        </Overlay>
    );
}

export default TransferCoinPage;
