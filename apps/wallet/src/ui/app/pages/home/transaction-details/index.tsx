// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getCertifiedTransaction,
    getExecutionStatusType,
    getTransactionKindName,
    getTransactions,
} from '@mysten/sui.js';
import clBind from 'classnames/bind';
import { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import Alert from '_components/alert';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Icon from '_components/icon';
import { useAppSelector } from '_hooks';
import { txSelectors } from '_redux/slices/transactions';

import type { TransactionKindName } from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';

import st from './TransactionDetailsPage.module.scss';

const cl = clBind.bind(st);

const txKindToTxt: Record<TransactionKindName, string> = {
    TransferObject: 'Object transfer',
    Call: 'Call',
    Publish: 'Publish',
    TransferSui: 'Sui transfer',
    ChangeEpoch: 'Change epoch',
    Pay: 'Pay',
    PaySui: 'PaySui',
    PayAllSui: 'PayAllSui',
};

function TransactionDetailsPage() {
    const { txDigest } = useParams();
    const txSelector = useMemo(
        () => (state: RootState) =>
            txDigest ? txSelectors.selectById(state, txDigest) : null,
        [txDigest]
    );
    // TODO: load tx if not found locally
    const txDetails = useAppSelector(txSelector);
    const status = txDetails && getExecutionStatusType(txDetails);
    const statusIcon = status === 'success' ? 'check2-circle' : 'x-circle';
    const transferKind =
        txDetails &&
        getTransactionKindName(
            // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
            getTransactions(getCertifiedTransaction(txDetails)!)[0]
        );
    return (
        <div className={cl('container')}>
            {txDetails ? (
                <>
                    <Icon className={cl('status', status)} icon={statusIcon} />
                    {transferKind ? (
                        <span className={cl('txt')}>
                            <strong>{txKindToTxt[transferKind]}</strong>{' '}
                            {status === 'success' ? 'was successful' : 'failed'}
                        </span>
                    ) : null}
                    {txDigest ? (
                        <ExplorerLink
                            className={cl('link')}
                            type={ExplorerLinkType.transaction}
                            transactionID={txDigest}
                            title="View on Sui Explorer"
                        />
                    ) : null}
                </>
            ) : (
                <Alert className={cl('error')}>
                    <strong>Transaction not found.</strong>{' '}
                    {txDigest ? (
                        <span>
                            Click{' '}
                            <ExplorerLink
                                type={ExplorerLinkType.transaction}
                                transactionID={txDigest}
                            >
                                here
                            </ExplorerLink>{' '}
                            to go to Sui Explorer.
                        </span>
                    ) : null}
                </Alert>
            )}
        </div>
    );
}

export default TransactionDetailsPage;
