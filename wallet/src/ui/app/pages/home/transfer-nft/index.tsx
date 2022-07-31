// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Formik } from 'formik';
import { useCallback, useMemo, useState } from 'react';
import { Navigate, useNavigate, useSearchParams } from 'react-router-dom';

import TransferNFTForm from './TransferNFTForm';
import { createValidationSchema } from './validation';
import BottomMenuLayout, {
    Content,
    Menu,
} from '_app/shared/bottom-menu-layout';
import Button from '_app/shared/button';
import PageTitle from '_app/shared/page-title';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import SuiObject from '_components/sui-object';
import { useAppSelector, useAppDispatch } from '_hooks';
import {
    accountAggregateBalancesSelector,
    accountNftsSelector,
} from '_redux/slices/account';
import { transferSuiNFT } from '_redux/slices/sui-objects';
import {
    GAS_TYPE_ARG,
    DEFAULT_NFT_TRANSFER_GAS_FEE,
} from '_redux/slices/sui-objects/Coin';

import type { SerializedError } from '@reduxjs/toolkit';
import type { FormikHelpers } from 'formik';

import st from './TransferNFTForm.module.scss';

const initialValues = {
    to: '',
    amount: DEFAULT_NFT_TRANSFER_GAS_FEE,
};

export type FormValues = typeof initialValues;

function TransferNFTPage() {
    const [searchParams] = useSearchParams();
    const objectId = useMemo(
        () => searchParams.get('objectId'),
        [searchParams]
    );
    const address = useAppSelector(({ account: { address } }) => address);

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
                        transferCost: DEFAULT_NFT_TRANSFER_GAS_FEE,
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
        <div className={st.container}>
            <PageTitle
                title="Send NFT"
                backLink="/nfts"
                className={st.pageTitle}
            />
            <BottomMenuLayout>
                <Content>
                    <div className={st.sendNft}>
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
                                    onClearSubmitError={
                                        handleOnClearSubmitError
                                    }
                                />
                            </Formik>
                            <SuiObject obj={selectedNFT} />
                        </Loading>
                    </div>
                </Content>
                <Menu stuckClass={st.shadow} className={st.shadow}>
                    <Button size="large" mode="primary" className={st.action}>
                        Stake Coins
                        <Icon
                            icon={SuiIcons.ArrowRight}
                            className={st.arrowActionIcon}
                        />
                    </Button>
                </Menu>
            </BottomMenuLayout>
        </div>
    );
}

export default TransferNFTPage;
