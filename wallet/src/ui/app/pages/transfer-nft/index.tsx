// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import TransferNFTForm from './TransferNFTForm';
import { createValidationSchema } from './validation';
import Loading from '_components/loading';
import SuiObject from '_components/sui-object';
import { useAppSelector, useAppDispatch } from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountNftsSelector,
} from '_redux/slices/account';
import { transferSuiNFT } from '_redux/slices/sui-objects';
import { GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

const initialValues = {
    to: '',
    amount: 10000,
};

export type FormValues = typeof initialValues;

function TransferNFTPage() {
    const [searchParams] = useSearchParams();
    const objectId = useMemo(
        () => searchParams.get('objectId'),
        [searchParams]
    );
    const address = useAppSelector(
        ({ account: { address } }) => address && `0x${address}`
    );

    let selectedNFT;
    const nftCollections = useAppSelector(accountNftsSelector);
    if (nftCollections && nftCollections.length) {
        selectedNFT = nftCollections.filter(
            (nftItems) => nftItems.reference.objectId === objectId
        )[0];
    }

    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);

    const gasAggregateBalance = useMemo(
        () => aggregateBalances[GAS_TYPE_ARG] || BigInt(0),
        [aggregateBalances]
    );

    const [sendError, setSendError] = useState<string | null>(null);

    const validationSchema = useMemo(
        () =>
            createValidationSchema(
                gasAggregateBalance,
                address || '',
                objectId || ''
            ),
        [gasAggregateBalance, address, objectId]
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
                        gasBalance={gasAggregateBalance.toString()}
                        onClearSubmitError={handleOnClearSubmitError}
                    />
                </Formik>
            </Loading>
        </>
    );
}

export default TransferNFTPage;
