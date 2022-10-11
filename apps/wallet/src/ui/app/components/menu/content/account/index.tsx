// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';

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
                <AccountAddress shorten={false} showLink={false} />
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
