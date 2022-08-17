// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

            return true;
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
                    title="Sign Message"
                    approveTitle="Sign Message"
                    rejectTitle="Reject"
                    origin={signMessageRequest.origin}
                    originFavIcon={signMessageRequest.originFavIcon}
                    onSubmit={handleOnSubmit}
                >
                    <h4 className={st.title}>Message</h4>
                    <pre className={st.message}>
                        {signMessageRequest.message}
                    </pre>
                </UserApproveContainer>
            )}
        </Loading>
    );
}
