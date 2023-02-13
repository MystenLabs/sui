// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Account24, ArrowUpRight12, Domain24, Version24 } from '@mysten/icons';
import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import LoadingIndicator from '../../loading/LoadingIndicator';
import MenuListItem from './MenuListItem';
import { API_ENV_TO_INFO } from '_app/ApiProvider';
import { Button } from '_app/shared/ButtonUI';
import { lockWallet } from '_app/wallet/actions';
import { useNextMenuUrl } from '_components/menu/hooks';
import { useAppDispatch, useAppSelector, useMiddleEllipsis } from '_hooks';
import { ToS_LINK } from '_src/shared/constants';
import { useAutoLockInterval } from '_src/ui/app/hooks/useAutoLockInterval';
import { logout } from '_src/ui/app/redux/slices/account';
import { Link } from '_src/ui/app/shared/Link';
import PageTitle from '_src/ui/app/shared/PageTitle';
import FaucetRequestButton from '_src/ui/app/shared/faucet/FaucetRequestButton';
import { Text } from '_src/ui/app/shared/text';

function MenuList() {
    const accountUrl = useNextMenuUrl(true, '/account');
    const networkUrl = useNextMenuUrl(true, '/network');
    const autoLockUrl = useNextMenuUrl(true, '/auto-lock');
    const address = useAppSelector(({ account }) => account.address);
    const shortenAddress = useMiddleEllipsis(address);
    const apiEnv = useAppSelector((state) => state.app.apiEnv);
    const networkName = API_ENV_TO_INFO[apiEnv].name;
    const autoLockInterval = useAutoLockInterval();
    const version = Browser.runtime.getManifest().version;
    const dispatch = useAppDispatch();
    const navigate = useNavigate();
    const [logoutInProgress, setLogoutInProgress] = useState(false);
    return (
        <>
            <PageTitle title="Wallet Settings" />
            <div className="flex flex-col divide-y divide-x-0 divide-solid divide-gray-45 mt-1.5">
                <MenuListItem
                    to={accountUrl}
                    icon={<Account24 />}
                    title="Account"
                    subtitle={shortenAddress}
                />
                <MenuListItem
                    to={networkUrl}
                    icon={<Domain24 />}
                    title="Network"
                    subtitle={networkName}
                />
                <MenuListItem
                    to={autoLockUrl}
                    icon={<Version24 />}
                    title="Auto-lock"
                    subtitle={
                        autoLockInterval ? (
                            `${autoLockInterval} min`
                        ) : (
                            <LoadingIndicator />
                        )
                    }
                />
            </div>
            <div className="flex flex-col items-stretch px-2.5">
                <FaucetRequestButton
                    variant="outline"
                    trackEventSource="settings"
                />
            </div>
            <div className="flex-1" />
            <div className="flex flex-nowrap flex-row items-stretch px-2.5 gap-3">
                <Button
                    variant="outline"
                    size="narrow"
                    onClick={async () => {
                        try {
                            await dispatch(lockWallet()).unwrap();
                            navigate('/locked', { replace: true });
                        } catch (e) {
                            // Do nothing
                        }
                    }}
                    text="Lock Wallet"
                />
                <Button
                    variant="outlineWarning"
                    text="Logout"
                    size="narrow"
                    loading={logoutInProgress}
                    onClick={async () => {
                        setLogoutInProgress(true);
                        try {
                            await dispatch(logout());
                            window.location.reload();
                        } finally {
                            setLogoutInProgress(false);
                        }
                    }}
                />
            </div>
            <div className="px-2.5 flex flex-col items-center justify-center no-underline gap-3.75 mt-1.25">
                <Link
                    href={ToS_LINK}
                    text="Terms of Service"
                    after={<ArrowUpRight12 />}
                />
                <Text variant="bodySmall" weight="medium" color="steel">
                    Wallet Version v{version}
                </Text>
            </div>
        </>
    );
}

export default MenuList;
