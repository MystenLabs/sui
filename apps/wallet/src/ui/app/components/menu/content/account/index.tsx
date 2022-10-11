// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';

import AutoLockTimerSelector from './auto-lock-timer-selector';
import { WALLET_ENCRYPTION_ENABLED } from '_app/wallet/constants';
import AccountAddress from '_components/account-address';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import Layout from '_components/menu/content/layout';
import { useNextMenuUrl } from '_components/menu/hooks';
import { useAppDispatch } from '_hooks';
import { logout } from '_redux/slices/account';

import st from './Account.module.scss';

function Account() {
    const backUrl = useNextMenuUrl(true, '/');
    const dispatch = useAppDispatch();
    const [logoutInProgress, setLogoutInProgress] = useState(false);
    const handleLogout = useCallback(async () => {
        setLogoutInProgress(true);
        try {
            await dispatch(logout());
        } finally {
            setLogoutInProgress(false);
        }
    }, [dispatch]);
    return (
        <Layout title="Account" backUrl={backUrl}>
            <div className={st.content}>
                <div className={st.itemGroup}>
                    <label className={st.itemTitle}>Address</label>
                    <AccountAddress shorten={true} showLink={false} />
                </div>
                {WALLET_ENCRYPTION_ENABLED ? (
                    <div className={st.itemGroup}>
                        <label className={st.itemTitle}>
                            Auto-lock timer (minutes)
                        </label>
                        <div className={st.itemDesc}>
                            Set the idle time in minutes before Sui Wallet locks
                            itself.
                        </div>
                        <AutoLockTimerSelector />
                    </div>
                ) : null}
            </div>
            <button
                className={st.logout}
                onClick={handleLogout}
                disabled={logoutInProgress}
            >
                {logoutInProgress ? (
                    <LoadingIndicator />
                ) : (
                    <>
                        <Icon
                            icon={SuiIcons.Logout}
                            className={st.logoutIcon}
                        />
                        Logout
                    </>
                )}
            </button>
        </Layout>
    );
}

export default Account;
