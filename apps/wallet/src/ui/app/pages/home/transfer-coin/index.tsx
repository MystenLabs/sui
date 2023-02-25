// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16, ArrowLeft16 } from '@mysten/icons';
import {
    Coin as CoinAPI,
    getTransactionDigest,
    SUI_TYPE_ARG,
} from '@mysten/sui.js';
import * as Sentry from '@sentry/react';
import { useMutation } from '@tanstack/react-query';
import { Formik, type FormikHelpers } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { toast } from 'react-hot-toast';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import { PreviewTransfer } from './PreviewTransfer';
import { SendTokenForm } from './SendTokenForm';
import { createValidationSchemaStepOne } from './validation';
import { Button } from '_app/shared/ButtonUI';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import { ActiveCoinsCard } from '_components/active-coins-card';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import Overlay from '_components/overlay';
import { parseAmount } from '_helpers';
import {
    useCoinDecimals,
    useSigner,
    useGetCoins,
    useAppSelector,
    useGasBudgetEstimationUnits,
} from '_hooks';
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

// Requesting object data for both SUI and the coins type, to estimate gas cost for the transaction for non SUI coins
// since caching is involved, for SUI coins the second request will be served from cache
function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const [showModal, setShowModal] = useState(true);
    const coinType = searchParams.get('type') || SUI_TYPE_ARG;
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
    const coins = useMemo(
        () => coinsData?.filter(({ lockedUntilEpoch }) => !lockedUntilEpoch),
        [coinsData]
    );

    const suiCoins = useMemo(
        () => suiCoinsData?.filter(({ lockedUntilEpoch }) => !lockedUntilEpoch),
        [suiCoinsData]
    );

    const coinBalance = useMemo(() => {
        return (
            coins?.reduce((acc, { balance }) => {
                return acc + BigInt(balance);
            }, 0n) || BigInt(0n)
        );
    }, [coins]);

    const gasAggregateBalance = useMemo(() => {
        return (
            suiCoins?.reduce((acc, { balance }) => {
                return acc + BigInt(balance);
            }, 0n) || BigInt(0n)
        );
    }, [suiCoins]);

    const coinSymbol = useMemo(
        () => (coinType && CoinAPI.getCoinSymbol(coinType)) || '',
        [coinType]
    );

    const [currentStep, setCurrentStep] = useState<number>(DEFAULT_FORM_STEP);
    const [formData, setFormData] = useState<FormValues>(initialValues);

    const [coinDecimals] = useCoinDecimals(coinType);
    const [gasDecimals] = useCoinDecimals(SUI_TYPE_ARG);
    const [amountToSend, setAmountToSend] = useState(BigInt(0));
    const maxSuiSingleCoinBalance = useMemo(() => {
        const maxCoin = suiCoins?.reduce(
            (max, { balance }) => (max < balance ? balance : max),
            0
        );
        return BigInt(maxCoin || 0);
    }, [suiCoins]);

    const gasBudgetEstimationUnits = useGasBudgetEstimationUnits(
        coins!,
        amountToSend
    );

    const {
        gasBudget: gasBudgetEstimation,
        isLoading: loadingBudgetEstimation,
    } = useGasBudgetInMist(gasBudgetEstimationUnits!);

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

    const navigate = useNavigate();

    const closeSendToken = useCallback(() => {
        navigate('/');
    }, [navigate]);

    const signer = useSigner();

    const transferCoin = async () => {
        const transaction = Sentry.startTransaction({ name: 'send-tokens' });
        try {
            if (
                coinType === null ||
                !gasBudgetEstimationUnits ||
                !signer ||
                !coins ||
                !suiCoins
            ) {
                throw new Error('Missing data');
            }
            trackEvent('TransferCoins', {
                props: { coinType },
            });

            // Use payAllSui if sendMax is true and the token type is SUI
            if (formData.isPayAllSui && coinType === SUI_TYPE_ARG) {
                return signer.payAllSui({
                    recipient: formData.to,
                    gasBudget: gasBudgetEstimationUnits,
                    inputCoins: suiCoins.map(
                        ({ coinObjectId }) => coinObjectId
                    ),
                });
            }

            const bigIntAmount = parseAmount(formData.amount, coinDecimals);

            // sort coins by balance
            const coinsIDs = coins
                .sort((a, b) => a.balance - b.balance)
                .map(({ coinObjectId }) => coinObjectId);

            return signer.signAndExecuteTransaction({
                kind: coinType === SUI_TYPE_ARG ? 'paySui' : 'pay',
                data: {
                    inputCoins: coinsIDs,
                    recipients: [formData.to],
                    amounts: [Number(bigIntAmount)],
                    gasBudget: Number(gasBudgetEstimationUnits),
                },
            });
        } catch (error) {
            transaction.setTag('failure', true);
            throw error;
        } finally {
            transaction.finish();
        }
    };

    const executeTransfer = useMutation({
        mutationFn: transferCoin,
        onSuccess: (response) => {
            const txDigest = getTransactionDigest(response);
            const receiptUrl = `/receipt?txdigest=${encodeURIComponent(
                txDigest
            )}&from=transactions`;
            return navigate(receiptUrl);
        },
        onError: (e) => {
            const errorMsg = (e as SerializedError).message || null;
            toast.error(
                <div className="max-w-xs overflow-hidden flex flex-col">
                    {errorMsg ? (
                        <small className="text-ellipsis overflow-hidden">
                            {errorMsg}
                        </small>
                    ) : null}
                </div>
            );
        },
    });

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

    const StepOne = (
        <>
            <div className="mb-7">
                <ActiveCoinsCard activeCoinType={coinType} />
            </div>

            <Formik
                initialValues={formData}
                validationSchema={validationSchemaStepOne}
                onSubmit={handleNextStep}
                enableReinitialize={true}
            >
                <SendTokenForm
                    coinType={coinType}
                    balance={coinBalance}
                    gasCostEstimation={gasBudgetEstimation || null}
                    onAmountChanged={(anAmount) => setAmountToSend(anAmount)}
                />
            </Formik>
        </>
    );

    const StepTwoPreview = (
        <BottomMenuLayout>
            <Content>
                <PreviewTransfer
                    coinType={coinType}
                    amount={formData.amount}
                    to={formData.to}
                    gasCostEstimation={gasBudgetEstimation || null}
                />
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
                    onClick={() => executeTransfer.mutateAsync()}
                    size="tall"
                    text={'Send Now'}
                    disabled={
                        coinType === null ||
                        !gasBudgetEstimationUnits ||
                        !coins ||
                        !suiCoins
                    }
                    after={<ArrowRight16 />}
                    loading={executeTransfer.isLoading}
                />
            </Menu>
        </BottomMenuLayout>
    );

    const steps = [StepOne, StepTwoPreview];

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title={currentStep === 1 ? 'Send Coins' : 'Review & Send'}
            closeOverlay={closeSendToken}
            closeIcon={SuiIcons.Close}
        >
            <Loading
                loading={
                    loadingBudgetEstimation ||
                    coinsIsLoading ||
                    suiCoinsIsLoading
                }
            >
                <div className="flex flex-col w-full mt-2.5">
                    {steps[currentStep - 1]}
                </div>
            </Loading>
        </Overlay>
    );
}

export default TransferCoinPage;
