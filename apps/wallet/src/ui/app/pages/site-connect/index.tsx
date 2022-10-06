// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo } from 'react';
import { useParams } from 'react-router-dom';

import Icon, { SuiIcons } from '_components/icon';
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

const permissionTypeToTxt: Record<PermissionType, string> = {
    viewAccount: 'Share wallet address',
    suggestTransactions: 'Suggest transactions to approve',
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
                        accounts: allowed ? [activeAccount] : [],
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
                    origin={permissionRequest.origin}
                    originFavIcon={permissionRequest.favIcon}
                    approveTitle="Connect"
                    rejectTitle="Cancel"
                    onSubmit={handleOnSubmit}
                    isConnect
                >
                    <div className={st.label}>App Permissions</div>
                    <ul className={st.permissions}>
                        {permissionRequest.permissions.map((aPermission) => (
                            <li key={aPermission} className={st.permission}>
                                <Icon
                                    icon={SuiIcons.Checkmark}
                                    className={st.checkmark}
                                />
                                {permissionTypeToTxt[aPermission]}
                            </li>
                        ))}
                    </ul>
                </UserApproveContainer>
            ) : null}
        </Loading>
    );
}

export default SiteConnectPage;
