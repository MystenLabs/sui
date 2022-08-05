// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { Formik } from 'formik';
import { useCallback, useMemo, useState, memo, useEffect } from 'react';
import { Link } from 'react-router-dom';

import TransferNFTForm from './TransferNFTForm';
import { createValidationSchema } from './validation';
import PageTitle from '_app/shared/page-title';
import Icon, { SuiIcons } from '_components/icon';
import NFTDisplayCard from '_components/nft-display';
import TxResponseCard from '_components/transaction-response-card';
import { useAppSelector, useAppDispatch } from '_hooks';
import {
    accountNftsSelector,
    accountAggregateBalancesSelector,
} from '_redux/slices/account';
import { setSelectedNFT, clearActiveNFT } from '_redux/slices/selected-nft';
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

type TxResponse = {
    address?: string;
    gasFee?: number;
    date?: number;
    status: 'success' | 'failure';
} | null;

const initTxResponse: TxResponse = null;

interface TransferProps {
    // nftObj?: SuiObject;
    objectId: ObjectId;
}

// Cache NFT object data before transfer of the NFT to use it in the TxResponse card
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

    useEffect(() => {
        dispatch(clearActiveNFT());
        if (selectedNFTObj) {
            dispatch(setSelectedNFT({ data: selectedNFTObj, loaded: true }));
        }
    }, [dispatch, objectId, selectedNFTObj]);

    const aggregateBalances = useAppSelector(accountAggregateBalancesSelector);
    const nftObj = useAppSelector(({ selectedNft }) => selectedNft);

    const gasAggregateBalance = useMemo(
        () => aggregateBalances[GAS_TYPE_ARG] || BigInt(0),
        [aggregateBalances]
    );

    const [txResponse, setTxResponse] = useState<TxResponse>(initTxResponse);
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

                setTxResponse((state) => ({
                    ...state,
                    address: to,
                    gasFee: resp.gasFee,
                    date: resp?.timestamp_ms,
                    status: resp.status === 'success' ? 'success' : 'failure',
                }));
                resetForm();
            } catch (e) {
                setSendError((e as SerializedError).message || null);
                setTxResponse((state) => ({
                    ...state,
                    address: to,
                    status: 'failure',
                }));
            }
        },
        [dispatch, objectId]
    );

    const handleOnClearSubmitError = useCallback(() => {
        setSendError(null);
    }, []);

    const TransferNFT = (
        <>
            <PageTitle
                title="Send NFT"
                backLink="/nfts"
                className={st.pageTitle}
            />
            <div className={st.content}>
                {nftObj.data && (
                    <NFTDisplayCard nftobj={nftObj.data} wideview={true} />
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
        </>
    );

    const TransferResponse = (
        <>
            {txResponse?.address ? (
                <div className={st.nftResponse}>
                    <TxResponseCard
                        status={txResponse.status}
                        address={txResponse.address}
                        date={
                            txResponse.date
                                ? new Date(txResponse.date).toDateString()
                                : null
                        }
                        errorMessage={sendError}
                        gasFee={txResponse.gasFee}
                    >
                        {nftObj.data && (
                            <NFTDisplayCard
                                nftobj={nftObj.data}
                                wideview={true}
                            />
                        )}
                    </TxResponseCard>
                    <div className={st.formcta}>
                        <Link
                            to="/nfts"
                            className={cl('btn', st.action, st.done, 'neutral')}
                        >
                            <Icon
                                icon={SuiIcons.Checkmark}
                                className={st.checkmark}
                            />
                            Done
                        </Link>
                    </div>
                </div>
            ) : (
                <></>
            )}
        </>
    );

    return (
        <div className={st.container}>
            {txResponse?.address ? TransferResponse : TransferNFT}
        </div>
    );
}

export default memo(TransferNFTCard);
