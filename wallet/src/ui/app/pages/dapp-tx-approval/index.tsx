// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, Fragment } from 'react';
import { useParams } from 'react-router-dom';

import Loading from '_components/loading';
import UserApproveContainer from '_components/user-approve-container';
import { useAppDispatch, useAppSelector, useInitializedGuard } from '_hooks';
import {
    respondToTransactionRequest,
    txRequestsSelectors,
} from '_redux/slices/transaction-requests';

import type { SuiJsonValue } from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';

import st from './DappTxApprovalPage.module.scss';
import stUserApprove from '_components/user-approve-container/UserApproveContainer.module.scss';

function toList(items: SuiJsonValue[]) {
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
        if (
            !loading &&
            (!txRequest || (txRequest && txRequest.approved !== null))
        ) {
            window.close();
        }
    }, [loading, txRequest]);
    // TODO: add more tx types/make it generic
    const valuesContent = useMemo(
        () =>
            txRequest?.type === 'move-call'
                ? [
                      { label: 'Transaction type', content: 'MoveCall' },
                      {
                          label: 'Package',
                          content: txRequest.tx.packageObjectId,
                      },
                      { label: 'Module', content: txRequest.tx.module },
                      { label: 'Function', content: txRequest.tx.function },
                      {
                          label: 'Arguments',
                          content: toList(txRequest.tx.arguments),
                      },
                      {
                          label: 'Type arguments',
                          content: toList(txRequest.tx.typeArguments),
                      },
                      { label: 'Gas budget', content: txRequest.tx.gasBudget },
                  ]
                : [
                      {
                          label: 'Transaction type',
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
                    title="Transaction Request"
                    origin={txRequest.origin}
                    originFavIcon={txRequest.originFavIcon}
                    approveTitle="Approve"
                    rejectTitle="Reject"
                    onSubmit={handleOnSubmit}
                >
                    {valuesContent.map(({ label, content }) => (
                        <Fragment key={label}>
                            <label className={stUserApprove.label}>
                                {label}
                            </label>
                            <div className={st.value}>{content}</div>
                        </Fragment>
                    ))}
                </UserApproveContainer>
            ) : null}
        </Loading>
    );
}
