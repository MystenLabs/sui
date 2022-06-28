// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { useIntl } from 'react-intl';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import TransferNFTForm from './TransferNFTForm';
import { createValidationSchema } from './validation';
import Loading from '_components/loading';
import SuiObject from '_components/sui-object';
import { useAppSelector, useAppDispatch } from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountItemizedBalancesSelector,
    accountNftsSelector,
} from '_redux/slices/account';
import { transferSuiNFT } from '_redux/slices/sui-objects';
import { Coin, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';
import { balanceFormatOptions } from '_shared/formatting';

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

const initialValues = {
    to: '',
    amount: '10000',
};

export type FormValues = typeof initialValues;

function TransferNFTPage() {
    const [searchParams] = useSearchParams();
    const objectId = useMemo(
        () => searchParams.get('objectId'),
        [searchParams]
    );

    let selectedNFT;
    const nftCollections = useAppSelector(accountNftsSelector);
    if (nftCollections && nftCollections.length) {
        selectedNFT = nftCollections.filter(
            (nftItems) => nftItems.reference.objectId === objectId
        )[0];
    }

    const balances = useAppSelector(accountItemizedBalancesSelector);

    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const coinTypes = useMemo(() => Object.keys(balances), [balances]);

    const coinBalance = useMemo(
        () => (objectId && aggregateBalances[coinTypes[0]]) || BigInt(0),
        [objectId, coinTypes, aggregateBalances]
    );

    const totalGasCoins = useMemo(
        () => balances[GAS_TYPE_ARG]?.length || 0,
        [balances]
    );

    const gasAggregateBalance = useMemo(
        () => aggregateBalances[GAS_TYPE_ARG] || BigInt(0),
        [aggregateBalances]
    );

    const coinSymbol = useMemo(
        () => (objectId && Coin.getCoinSymbol(objectId)) || '',
        [objectId]
    );
    const [sendError, setSendError] = useState<string | null>(null);
    const intl = useIntl();
    const validationSchema = useMemo(
        () =>
            createValidationSchema(
                objectId || '',
                coinBalance,
                coinSymbol,
                gasAggregateBalance,
                totalGasCoins,
                intl,
                balanceFormatOptions
            ),
        [
            objectId,
            coinBalance,
            coinSymbol,
            gasAggregateBalance,
            totalGasCoins,
            intl,
        ]
    );
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const onHandleSubmit = useCallback(
        async (
            { to }: FormValues,
            { resetForm }: FormikHelpers<FormValues>
        ) => {
            if (objectId === null) {
                return;
            }
            setSendError(null);
            try {
                await dispatch(
                    transferSuiNFT({
                        recipientAddress: to,
                        nftId: objectId,
                    })
                ).unwrap();
                resetForm();
                navigate('/nfts/');
            } catch (e) {
                setSendError((e as SerializedError).message || null);
            }
        },
        [dispatch, navigate, objectId]
    );
    const handleOnClearSubmitError = useCallback(() => {
        setSendError(null);
    }, []);
    const loadingBalance = useAppSelector(
        ({ suiObjects }) => suiObjects.loading && !suiObjects.lastSync
    );

    if (!objectId || !selectedNFT) {
        return <Navigate to="/nfts" replace={true} />;
    }

    return (
        <>
            <h3>Send This NFT</h3>
            <SuiObject obj={selectedNFT} />
            <br />
            <Loading loading={loadingBalance}>
                <Formik
                    initialValues={initialValues}
                    validateOnMount={true}
                    validationSchema={validationSchema}
                    onSubmit={onHandleSubmit}
                >
                    <TransferNFTForm
                        submitError={sendError}
                        coinBalance={coinBalance.toString()}
                        coinSymbol={coinSymbol}
                        onClearSubmitError={handleOnClearSubmitError}
                    />
                </Formik>
            </Loading>
        </>
    );
}

export default TransferNFTPage;
