// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getTransactionKindName,
    getTransactions,
} from '@mysten/sui.js';
import clBind from 'classnames/bind';
import { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import Alert from '_components/alert';
import BsIcon from '_components/bs-icon';
import { useAppSelector } from '_hooks';
import { Explorer } from '_redux/slices/sui-objects/Explorer';
import { txSelectors } from '_redux/slices/transactions';

import type { TransactionKindName } from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';

import st from './TransactionDetailsPage.module.scss';

const cl = clBind.bind(st);

const txKindToTxt: Record<TransactionKindName, string> = {
    TransferCoin: 'Coin transfer',
    Call: 'Call',
    Publish: 'Publish',
};

function TransactionDetailsPage() {
    const { txDigest } = useParams();
    const txSelector = useMemo(
        () => (state: RootState) =>
            txDigest ? txSelectors.selectById(state, txDigest) : null,
        [txDigest]
    );
    const explorerLink = useMemo(
        () => (txDigest ? Explorer.getTransactionUrl(txDigest) : null),
        [txDigest]
    );
    // TODO: load tx if not found locally
    const txDetails = useAppSelector(txSelector);
    const status =
        txDetails && getExecutionStatusType(txDetails.EffectResponse);
    const statusIcon = status === 'success' ? 'check2-circle' : 'x-circle';
    const transferKind =
        txDetails &&
        getTransactionKindName(
            getTransactions(txDetails.EffectResponse.certificate)[0]
        );
    return (
        <div className={cl('container')}>
            {txDetails ? (
                <>
                    <BsIcon
                        className={cl('status', status)}
                        icon={statusIcon}
                    />
                    {transferKind ? (
                        <span className={cl('txt')}>
                            <strong>{txKindToTxt[transferKind]}</strong>{' '}
                            {status === 'success' ? 'was successful' : 'failed'}
                        </span>
                    ) : null}
                    {explorerLink ? (
                        <a
                            className={cl('link')}
                            href={explorerLink}
                            target="_blank"
                            rel="noreferrer"
                            title="View on Sui Explorer"
                        >
                            <BsIcon icon="box-arrow-up-right" />
                        </a>
                    ) : null}
                </>
            ) : (
                <Alert className={cl('error')}>
                    <strong>Transaction not found.</strong>{' '}
                    {explorerLink ? (
                        <span>
                            Click{' '}
                            <a
                                href={explorerLink}
                                target="_blank"
                                rel="noreferrer"
                            >
                                here
                            </a>{' '}
                            to go to Sui Explorer.
                        </span>
                    ) : null}
                </Alert>
            )}
        </div>
    );
}

export default TransactionDetailsPage;
