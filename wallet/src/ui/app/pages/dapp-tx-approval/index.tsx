// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useCallback, useEffect, useMemo, Fragment } from 'react';
import { useParams } from 'react-router-dom';

import Loading from '_components/loading';
import UserApproveContainer from '_components/user-approve-container';
import { useAppDispatch, useAppSelector, useInitializedGuard } from '_hooks';
import {
    loadTransactionResponseMetadata,
    respondToTransactionRequest,
    txRequestsSelectors,
} from '_redux/slices/transaction-requests';

import type { CallArg, SuiJsonValue, TypeTag } from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';

import st from './DappTxApprovalPage.module.scss';

function toList(items: SuiJsonValue[] | TypeTag[] | CallArg[]) {
    if (!items.length) {
        return '-';
    }
    return (
        <ul className={st.list}>
            {items.map((anItem) => {
                const val = JSON.stringify(anItem, null, 4);
                return <li key={val}>{val}</li>;
            })}
        </ul>
    );
}

export function DappTxApprovalPage() {
    const { txID } = useParams();
    const guardLoading = useInitializedGuard(true);
    const txRequestsLoading = useAppSelector(
        ({ transactionRequests }) => !transactionRequests.initialized
    );
    const txRequestSelector = useMemo(
        () => (state: RootState) =>
            (txID && txRequestsSelectors.selectById(state, txID)) || null,
        [txID]
    );
    const txRequest = useAppSelector(txRequestSelector);
    const loading = guardLoading || txRequestsLoading;
    const dispatch = useAppDispatch();
    const handleOnSubmit = useCallback(
        async (approved: boolean) => {
            if (txRequest) {
                await dispatch(
                    respondToTransactionRequest({
                        approved,
                        txRequestID: txRequest.id,
                    })
                );
            }
        },
        [dispatch, txRequest]
    );

    useEffect(() => {
        console.log('LOADING METADATA');
        if (txRequest?.type === 'move-call' && !txRequest.metadata) {
            dispatch(
                loadTransactionResponseMetadata({
                    txRequestID: txRequest.id,
                    objectId: txRequest.tx.packageObjectId,
                    moduleName: txRequest.tx.module,
                    functionName: txRequest.tx.module,
                })
            );
        }
    }, [txRequest, dispatch]);

    console.log(txRequest);

    useEffect(() => {
        if (
            !loading &&
            (!txRequest || (txRequest && txRequest.approved !== null))
        ) {
            window.close();
        }
    }, [loading, txRequest]);

    const valuesContent = useMemo(
        () =>
            txRequest?.type === 'move-call'
                ? [
                      { label: 'Transaction Type', content: 'MoveCall' },
                      { label: 'Function', content: txRequest.tx.function },
                      { label: 'Gas Fees', content: txRequest.tx.gasBudget },
                  ]
                : [
                      {
                          label: 'Transaction Type',
                          content: 'SerializedMoveCall',
                      },
                      { label: 'Contents', content: txRequest?.txBytes },
                  ],
        [txRequest]
    );

    return (
        <Loading loading={loading}>
            {txRequest ? (
                <UserApproveContainer
                    origin={txRequest.origin}
                    originFavIcon={txRequest.originFavIcon}
                    approveTitle="Approve"
                    rejectTitle="Reject"
                    onSubmit={handleOnSubmit}
                >
                    <dl className={st.card}>
                        <div className={st.content}>
                            {valuesContent.map(({ label, content }) => (
                                <div key={label} className={st.row}>
                                    <dt>{label}</dt>
                                    <dd>{content}</dd>
                                </div>
                            ))}
                        </div>
                    </dl>
                    <div className={st.tabs}>
                        <button className={cl(st.tab, st.active)}>
                            Transfer
                        </button>
                        <button className={cl(st.tab)}>Modify</button>
                        <button className={cl(st.tab)}>Read</button>
                    </div>
                </UserApproveContainer>
            ) : null}
        </Loading>
    );
}
