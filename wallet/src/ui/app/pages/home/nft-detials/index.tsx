// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import 
import { isSuiMoveObject } from '@mysten/sui.js';
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
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import NFTdisplay from '_components/nft-display';
import { useAppSelector, useAppDispatch, useMiddleEllipsis } from '_hooks';
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

import st from './NFTDetails.module.scss';

const initialValues = {
    to: '',
    amount: DEFAULT_NFT_TRANSFER_GAS_FEE,
};

export type FormValues = typeof initialValues;

function NFTDetialsPage() {
    const [searchParams] = useSearchParams();
    const objectId = useMemo(
        () => searchParams.get('objectId'),
        [searchParams]
    );
    const address = useAppSelector(({ account: { address } }) => address);
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    let selectedNFT;
    let nftFields;
    const nftCollections = useAppSelector(accountNftsSelector);
    if (nftCollections && nftCollections.length) {
        selectedNFT = nftCollections.filter(
            (nftItems) => nftItems.reference.objectId === objectId
        )[0];
    }

    if (selectedNFT) {
        nftFields = isSuiMoveObject(selectedNFT.data)
            ? selectedNFT.data.fields
            : null;
    }
    console.log(nftFields);

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

    const SendNFT = (
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
    );

    const NFTDetails = nftFields && (
        <div className={st.nftDetails}>
            <div>Object ID</div>{' '}
            <div>
                {' '}
                <ExplorerLink
                    type={ExplorerLinkType.address}
                    address={nftFields.info.id}
                    title="View on Sui Explorer"
                    className={st.explorerLink}
                >
                    {nftFields.info.id}
                </ExplorerLink>
            </div>
        </div>
    );

    return (
        <div className={st.container}>
            <PageTitle
                title={nftFields?.name}
                backLink="/nfts"
                className={st.pageTitle}
            />
            <BottomMenuLayout>
                <Content>
                    <section className={st.nftDetail}>
                        <NFTdisplay nftobj={selectedNFT} size="large" />
                        {NFTDetails}
                        <div className={st.sendNftForm}>{SendNFT}</div>
                    </section>
                </Content>
                <Menu stuckClass={st.shadow} className={st.shadow}>
                    <Button size="large" mode="neutral" className={st.action}>
                        <Icon
                            icon={SuiIcons.Close}
                            className={st.closeActionIcon}
                        />
                        Cancel
                    </Button>
                    <Button size="large" mode="primary" className={st.action}>
                        Send NFT
                        <Icon
                            icon={SuiIcons.ArrowRight}
                            className={cl(st.arrowActionIcon)}
                        />
                    </Button>
                </Menu>
            </BottomMenuLayout>
        </div>
    );
}

export default NFTDetialsPage;
