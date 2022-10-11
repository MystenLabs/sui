// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Formik } from 'formik';
import { useCallback, useMemo, useState, memo } from 'react';
import { useNavigate } from 'react-router-dom';

import TransferNFTForm from './TransferNFTForm';
import { createValidationSchema } from './validation';
import PageTitle from '_app/shared/page-title';
import NFTDisplayCard from '_components/nft-display';
import { useAppSelector, useAppDispatch } from '_hooks';
import {
    accountNftsSelector,
    accountAggregateBalancesSelector,
} from '_redux/slices/account';
import { transferSuiNFT } from '_redux/slices/sui-objects';
import {
    GAS_TYPE_ARG,
    DEFAULT_NFT_TRANSFER_GAS_FEE,
} from '_redux/slices/sui-objects/Coin';

import type { ObjectId } from '@mysten/sui.js';
import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

import st from './TransferNFTForm.module.scss';

const initialValues = {
    to: '',
    amount: DEFAULT_NFT_TRANSFER_GAS_FEE,
};

export type FormValues = typeof initialValues;

interface TransferProps {
    objectId: ObjectId;
}

function TransferNFTCard({ objectId }: TransferProps) {
    const address = useAppSelector(({ account: { address } }) => address);
    const dispatch = useAppDispatch();

    const nftCollections = useAppSelector(accountNftsSelector);
    const selectedNFTObj = useMemo(
        () =>
            nftCollections.filter(
                (nftItems) => nftItems.reference.objectId === objectId
            )[0],
        [nftCollections, objectId]
    );

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
                const resp = await dispatch(
                    transferSuiNFT({
                        recipientAddress: to,
                        nftId: objectId,
                        transferCost: DEFAULT_NFT_TRANSFER_GAS_FEE,
                    })
                ).unwrap();

                resetForm();
                if (resp.txId) {
                    navigate(
                        `/receipt?${new URLSearchParams({
                            txdigest: resp.txId,
                            transfer: 'nft',
                        }).toString()}`
                    );
                }
            } catch (e) {
                setSendError((e as SerializedError).message || null);
            }
        },
        [dispatch, navigate, objectId]
    );

    const handleOnClearSubmitError = useCallback(() => {
        setSendError(null);
    }, []);

    return (
        <div className={st.container}>
            <PageTitle
                title="Send NFT"
                backLink="/nfts"
                className={st.pageTitle}
                hideBackLabel={true}
            />
            <div className={st.content}>
                {selectedNFTObj && (
                    <NFTDisplayCard nftobj={selectedNFTObj} wideview={true} />
                )}
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
            </div>
        </div>
    );
}

export default memo(TransferNFTCard);
