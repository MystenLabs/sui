// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getObjectId,
    hasPublicTransfer,
    is,
    SuiObject,
    getObjectOwner,
} from '@mysten/sui.js';
import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate, useParams } from 'react-router-dom';

import TransferNFTForm from './TransferNFTForm';
import { createValidationSchema } from './validation';
import { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import { NFTDisplayCard } from '_components/nft-display';
import Overlay from '_components/overlay';
import { useAppSelector, useAppDispatch, useGetObject } from '_hooks';
import { transferNFT } from '_redux/slices/sui-objects';
import { DEFAULT_NFT_TRANSFER_GAS_FEE } from '_redux/slices/sui-objects/Coin';

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

const initialValues = {
    to: '',
};

export type FormValues = typeof initialValues;

function NftTransferPage() {
    const { nftId } = useParams();
    const address = useAppSelector(({ account: { address } }) => address);
    const [showModal, setShowModal] = useState(true);

    // verify that the nft is owned by the user and is trnasferable
    const { data: objectData, isLoading } = useGetObject(nftId!);
    const selectedNft = useMemo(() => {
        if (
            !is(objectData?.details, SuiObject) ||
            !objectData ||
            !hasPublicTransfer(objectData.details)
        )
            return null;
        const owner = getObjectOwner(objectData) as { AddressOwner: string };
        return owner.AddressOwner === address ? objectData.details : null;
    }, [address, objectData]);

    const objectId = selectedNft ? getObjectId(selectedNft.reference) : null;

    const dispatch = useAppDispatch();
    const [sendError, setSendError] = useState<string | null>(null);
    const validationSchema = useMemo(
        () => createValidationSchema(address!, objectId!),
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
                        recipient: to,
                        objectId,
                        gasBudget: DEFAULT_NFT_TRANSFER_GAS_FEE,
                    })
                ).unwrap();
                resetForm();
                if (resp.txId) {
                    navigate(
                        `/receipt?${new URLSearchParams({
                            txdigest: resp.txId,
                            from: 'nfts',
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

    const closeSendToken = useCallback(() => {
        navigate('/');
    }, [navigate]);

    return (
        <Overlay
            showModal={showModal}
            setShowModal={setShowModal}
            title="Send NFT"
            closeOverlay={closeSendToken}
            closeIcon={SuiIcons.Close}
        >
            <div className="flex w-full flex-col">
                <Loading loading={isLoading}>
                    {objectId && nftId ? (
                        <>
                            <div className="mb-7.5">
                                <NFTDisplayCard
                                    objectId={nftId}
                                    wideView
                                    size="sm"
                                />
                            </div>
                            <Formik
                                initialValues={initialValues}
                                validateOnMount={true}
                                validationSchema={validationSchema}
                                onSubmit={onHandleSubmit}
                            >
                                <TransferNFTForm
                                    submitError={sendError}
                                    gasBudget={DEFAULT_NFT_TRANSFER_GAS_FEE}
                                    onClearSubmitError={
                                        handleOnClearSubmitError
                                    }
                                />
                            </Formik>
                        </>
                    ) : (
                        <Navigate to="/" replace />
                    )}
                </Loading>
            </div>
        </Overlay>
    );
}

export default NftTransferPage;
