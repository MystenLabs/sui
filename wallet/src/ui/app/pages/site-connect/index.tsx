// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useState } from 'react';
import { useParams } from 'react-router-dom';

import AccountAddress from '_components/account-address';
import Loading from '_components/loading';
import { useAppDispatch, useAppSelector, useInitializedGuard } from '_hooks';
import {
    permissionsSelectors,
    respondToPermissionRequest,
} from '_redux/slices/permissions';

import type { PermissionType } from '_messages/payloads/permissions';
import type { RootState } from '_redux/RootReducer';
import type { MouseEventHandler } from 'react';

import st from './SiteConnectPage.module.scss';

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
    const [submitting, setSubmitting] = useState(false);
    const handleOnResponse = useCallback<MouseEventHandler<HTMLButtonElement>>(
        (e) => {
            const allowed = e.currentTarget.dataset.allow === 'true';
            if (requestID && activeAccount) {
                setSubmitting(true);
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
                <div className={st.container}>
                    <h2 className={st.title}>Connect to Sui wallet</h2>
                    <label className={st.label}>Site</label>
                    <div className={st.originContainer}>
                        {permissionRequest.favIcon ? (
                            <img
                                className={st.favIcon}
                                src={permissionRequest.favIcon}
                                alt="Site favicon"
                            />
                        ) : null}
                        <span className={st.origin}>
                            {permissionRequest.origin}
                        </span>
                    </div>
                    <label className={st.label}>Account</label>
                    <AccountAddress showLink={false} />
                    <label className={st.label}>Permissions</label>
                    <div className={st.permissionsContainer}>
                        {permissionRequest.permissions.map((aPermission) => (
                            <span className={st.permission} key={aPermission}>
                                {permissionTypeToTxt[aPermission]}
                            </span>
                        ))}
                    </div>
                    <div className={st.actions}>
                        <button
                            type="button"
                            data-allow="false"
                            onClick={handleOnResponse}
                            className="btn link"
                            disabled={submitting}
                        >
                            Cancel
                        </button>
                        <button
                            type="button"
                            className="btn"
                            data-allow="true"
                            onClick={handleOnResponse}
                            disabled={submitting}
                        >
                            Connect
                        </button>
                    </div>
                </div>
            ) : null}
        </Loading>
    );
}

export default SiteConnectPage;
