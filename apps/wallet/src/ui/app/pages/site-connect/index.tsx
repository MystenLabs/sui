// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import Icon, { SuiIcons } from '_components/icon';
import Loading from '_components/loading';
import UserApproveContainer from '_components/user-approve-container';
import { useAppDispatch, useAppSelector } from '_hooks';
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
    const permissionsInitialized = useAppSelector(
        ({ permissions }) => permissions.initialized
    );
    const loading = !permissionsInitialized;
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

    const parsedOrigin = useMemo(
        () => (permissionRequest ? new URL(permissionRequest.origin) : null),
        [permissionRequest]
    );

    const isSecure = parsedOrigin?.protocol === 'https:';
    const [displayWarning, setDisplayWarning] = useState(!isSecure);

    const handleHideWarning = useCallback(
        (allowed: boolean) => {
            if (allowed) {
                setDisplayWarning(false);
            } else {
                handleOnSubmit(false);
            }
        },
        [handleOnSubmit]
    );

    useEffect(() => {
        setDisplayWarning(!isSecure);
    }, [isSecure]);

    return (
        <Loading loading={loading}>
            {permissionRequest &&
                (displayWarning ? (
                    <UserApproveContainer
                        origin={permissionRequest.origin}
                        originFavIcon={permissionRequest.favIcon}
                        approveTitle="Continue"
                        rejectTitle="Reject"
                        onSubmit={handleHideWarning}
                        isWarning
                        isConnect
                    >
                        <div className={st.warningWrapper}>
                            <h1 className={st.warningTitle}>
                                Your Connection is Not Secure
                            </h1>
                        </div>

                        <div className={st.warningMessage}>
                            This site requesting this wallet connection is not
                            secure, and attackers might be trying to steal your
                            information.
                            <br />
                            <br />
                            Continue at your own risk.
                        </div>
                    </UserApproveContainer>
                ) : (
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
                            {permissionRequest.permissions.map(
                                (aPermission) => (
                                    <li
                                        key={aPermission}
                                        className={st.permission}
                                    >
                                        <Icon
                                            icon={SuiIcons.Checkmark}
                                            className={st.checkmark}
                                        />
                                        {permissionTypeToTxt[aPermission]}
                                    </li>
                                )
                            )}
                        </ul>
                    </UserApproveContainer>
                ))}
        </Loading>
    );
}

export default SiteConnectPage;
