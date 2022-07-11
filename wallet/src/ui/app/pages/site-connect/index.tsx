// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo } from 'react';
import { useParams } from 'react-router-dom';

import Loading from '_components/loading';
import UserApproveContainer from '_components/user-approve-container';
import { useAppDispatch, useAppSelector, useInitializedGuard } from '_hooks';
import {
    permissionsSelectors,
    respondToPermissionRequest,
} from '_redux/slices/permissions';

import type { PermissionType } from '_messages/payloads/permissions';
import type { RootState } from '_redux/RootReducer';

import st from './SiteConnectPage.module.scss';
import stUserApprove from '_components/user-approve-container/UserApproveContainer.module.scss';

const permissionTypeToTxt: Record<PermissionType, string> = {
    viewAccount: 'View Account',
    suggestTransactions: 'Propose transactions',
};

function SiteConnectPage() {
    const { requestID } = useParams();
    const guardLoading = useInitializedGuard(true);
    const permissionsInitialized = useAppSelector(
        ({ permissions }) => permissions.initialized
    );
    const loading = guardLoading || !permissionsInitialized;
    const permissionSelector = useMemo(
        () => (state: RootState) =>
            requestID
                ? permissionsSelectors.selectById(state, requestID)
                : null,
        [requestID]
    );
    const dispatch = useAppDispatch();
    const permissionRequest = useAppSelector(permissionSelector);
    const activeAccount = useAppSelector(({ account }) => account.address);
    const handleOnSubmit = useCallback(
        (allowed: boolean) => {
            if (requestID && activeAccount) {
                dispatch(
                    respondToPermissionRequest({
                        id: requestID,
                        accounts: allowed ? [`0x${activeAccount}`] : [],
                        allowed,
                    })
                );
            }
        },
        [dispatch, requestID, activeAccount]
    );
    useEffect(() => {
        if (
            !loading &&
            (!permissionRequest || permissionRequest.responseDate)
        ) {
            window.close();
        }
    }, [loading, permissionRequest]);

    return (
        <Loading loading={loading}>
            {permissionRequest ? (
                <UserApproveContainer
                    title="Connect to Sui wallet"
                    origin={permissionRequest.origin}
                    originFavIcon={permissionRequest.favIcon}
                    approveTitle="Connect"
                    rejectTitle="Cancel"
                    onSubmit={handleOnSubmit}
                >
                    <label className={stUserApprove.label}>Permissions</label>
                    <div className={st.permissionsContainer}>
                        {permissionRequest.permissions.map((aPermission) => (
                            <span className={st.permission} key={aPermission}>
                                {permissionTypeToTxt[aPermission]}
                            </span>
                        ))}
                    </div>
                </UserApproveContainer>
            ) : null}
        </Loading>
    );
}

export default SiteConnectPage;
