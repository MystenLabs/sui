// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16, ArrowLeft16 } from '@mysten/icons';
import {
    Coin as CoinAPI,
    getTransactionDigest,
    SUI_TYPE_ARG,
    type SuiMoveObject,
    getObjectExistsResponse,
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
    useGetCoinObjectsByCoinType,
} from '_hooks';
import {} from '_redux/slices/account';
import { Coin } from '_redux/slices/sui-objects/Coin';
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

// Requesting object data for both SUI and the coin type, to estimate gas cost for the transaction for non SUI coins
// since caching is involved, for SUI coins the second request will be served from cache
function TransferCoinPage() {
    const [searchParams] = useSearchParams();
    const [showModal, setShowModal] = useState(true);
    const coinType = searchParams.get('type') || SUI_TYPE_ARG;

    // Get all coins of the type
    const { data: coinsData, isLoading: coinsIsLoading } =
        useGetCoinObjectsByCoinType(coinType);

    // Get SUI balance
    const { data: suiCoinsData, isLoading: suiCoinsIsLoading } =
        useGetCoinObjectsByCoinType(SUI_TYPE_ARG);

    const coins = useMemo(() => {
        const coinsExists = coinsData?.filter(
            ({ status }) => status === 'Exists'
        );
        return (
            coinsExists?.map(
                (data) => getObjectExistsResponse(data)?.data as SuiMoveObject
            ) || null
        );
    }, [coinsData]);

    const suiCoins = useMemo(() => {
        const coins = suiCoinsData?.filter(({ status }) => status === 'Exists');
        return (
            coins?.map(
                (data) => getObjectExistsResponse(data)?.data as SuiMoveObject
            ) || null
        );
    }, [suiCoinsData]);

    const coinBalance = useMemo(() => {
        return (
            coins?.reduce((acc, { fields }) => {
                return acc + BigInt(fields.balance || 0);
            }, 0n) || BigInt(0n)
        );
    }, [coins]);

    const gasAggregateBalance = useMemo(() => {
        return (
            suiCoins?.reduce((acc, { fields }) => {
                return acc + BigInt(fields.balance || 0);
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
    const maxSuiSingleCoinBalance = useMemo(
        () =>
            suiCoins?.reduce(
                (max, { fields }) =>
                    max < fields.balance ? fields.balance : max,
                BigInt(0)
            ) || BigInt(0),
        [suiCoins]
    );
    const gasBudgetEstimationUnits = useMemo(
        () => (coins ? Coin.computeGasBudgetForPay(coins, amountToSend) : null),
        [amountToSend, coins]
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
                !coins ||
                !suiCoins ||
                !signer
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
                    inputCoins: suiCoins.map((coin) => CoinAPI.getID(coin)),
                });
            }

            const bigIntAmount = parseAmount(formData.amount, coinDecimals);
            return signer.signAndExecuteTransaction(
                await CoinAPI.newPayTransaction(
                    coins,
                    coinType,
                    bigIntAmount,
                    formData.to,
                    gasBudgetEstimationUnits
                )
            );
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
