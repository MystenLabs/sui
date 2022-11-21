// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectId, hasPublicTransfer } from '@mysten/sui.js';
import cl from 'classnames';
import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate, useParams } from 'react-router-dom';

import TransferNFTForm from './TransferNFTForm';
import { createValidationSchema } from './validation';
import PageTitle from '_app/shared/page-title';
import Loading from '_components/loading';
import NFTDisplayCard from '_components/nft-display';
import { useAppSelector, useAppDispatch, useObjectsState } from '_hooks';
import { createAccountNftByIdSelector } from '_redux/slices/account';
import { transferNFT } from '_redux/slices/sui-objects';

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

import st from './TransferNFTForm.module.scss';

const initialValues = {
    to: '',
};

export type FormValues = typeof initialValues;

function NftTransferPage() {
    const { nftId } = useParams();
    const nftSelector = useMemo(
        () => createAccountNftByIdSelector(nftId || ''),
        [nftId]
    );
    const selectedNft = useAppSelector(nftSelector);
    const objectId = selectedNft ? getObjectId(selectedNft.reference) : null;
    const address = useAppSelector(({ account: { address } }) => address);
    const dispatch = useAppDispatch();
    const [sendError, setSendError] = useState<string | null>(null);
    const validationSchema = useMemo(
        () => createValidationSchema(address || '', objectId || ''),
        [address, objectId]
    );
    const navigate = useNavigate();
    const onHandleSubmit = useCallback(
        async (
            { to }: FormValues,
            { resetForm }: FormikHelpers<FormValues>
        ) => {
            if (!objectId) {
                return;
            }
            setSendError(null);
            try {
                const resp = await dispatch(
                    transferNFT({
                        recipientAddress: to,
                        nftId: objectId,
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
    const { loading } = useObjectsState();
    return (
        <div className={cl(st.container, { 'items-center': loading })}>
            <Loading loading={loading}>
                {selectedNft && objectId && hasPublicTransfer(selectedNft) ? (
                    <>
                        <PageTitle
                            title="Send NFT"
                            backLink={`/nft-details?${new URLSearchParams({
                                objectId,
                            }).toString()}`}
                            className={st.pageTitle}
                            hideBackLabel={true}
                        />
                        <div className={st.content}>
                            <NFTDisplayCard
                                nftobj={selectedNft}
                                wideview={true}
                            />

                            <Formik
                                initialValues={initialValues}
                                validateOnMount={true}
                                validationSchema={validationSchema}
                                onSubmit={onHandleSubmit}
                            >
                                <TransferNFTForm
                                    nftID={objectId}
                                    submitError={sendError}
                                    onClearSubmitError={
                                        handleOnClearSubmitError
                                    }
                                />
                            </Formik>
                        </div>
                    </>
                ) : (
                    <Navigate to="/" replace />
                )}
            </Loading>
        </div>
    );
}

export default NftTransferPage;
