// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useCallback, useEffect, useMemo } from 'react';
import { useParams } from 'react-router-dom';

import Loading from '_components/loading';
import UserApproveContainer from '_components/user-approve-container';
import { useAppDispatch, useAppSelector, useInitializedGuard } from '_hooks';
import {
    respondToSignMessageRequest,
    signMessageRequestsSelectors,
} from '_redux/slices/sign-message-requests';

import type { RootState } from '_redux/RootReducer';

import st from './DappSignMessageApprovalPage.module.scss';

export function DappSignMessageApprovalPage() {
    const { signMessageRequestID } = useParams();
    const guardLoading = useInitializedGuard(true);
    const signMessageRequestLoading = useAppSelector(
        ({ signMessageRequests }) => !signMessageRequests.initialized
    );
    const signMessageRequestSelector = useMemo(
        () => (state: RootState) =>
            (signMessageRequestID &&
                signMessageRequestsSelectors.selectById(
                    state,
                    signMessageRequestID
                )) ||
            null,
        [signMessageRequestID]
    );
    const signMessageRequest = useAppSelector(signMessageRequestSelector);
    const loading = guardLoading || signMessageRequestLoading;
    const dispatch = useAppDispatch();

    const handleOnSubmit = useCallback(
        async (approved: boolean) => {
            if (signMessageRequest) {
                await dispatch(
                    respondToSignMessageRequest({
                        approved,
                        id: signMessageRequest.id,
                    })
                );
            }
        },
        [dispatch, signMessageRequest]
    );

    useEffect(() => {
        if (
            !loading &&
            (!signMessageRequest ||
                (signMessageRequest && signMessageRequest.approved !== null))
        ) {
            window.close();
        }
    }, [loading, signMessageRequest]);

    return (
        <Loading loading={loading}>
            {signMessageRequest && (
                <UserApproveContainer
                    approveTitle="Sign"
                    rejectTitle="Reject"
                    origin={signMessageRequest.origin}
                    originFavIcon={signMessageRequest.originFavIcon}
                    onSubmit={handleOnSubmit}
                >
                    <div className={st.card}>
                        <div className={st.content}>
                            <h2>Sign Message Request</h2>
                        </div>
                    </div>
                    <div className={st.tabs}>
                        <button type="button" className={cl(st.tab, st.active)}>
                            Message Contents
                        </button>
                    </div>
                    <div className={st.message}>
                        {signMessageRequest.messageString ||
                            signMessageRequest.messageData}
                        {!signMessageRequest.messageString && (
                            <small>{' (base64)'}</small>
                        )}
                    </div>
                </UserApproveContainer>
            )}
        </Loading>
    );
}
