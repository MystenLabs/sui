// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useCallback, useEffect, useState } from 'react';

import Alert from '_components/alert';
import BsIcon from '_components/bs-icon';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import ExternalLink from '_components/external-link';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import Network from '_components/network-switch';
import { useAppDispatch } from '_hooks';
import { logout } from '_redux/slices/account';
import { mintDemoNFT } from '_redux/slices/sui-objects';
import { ToS_LINK } from '_shared/constants';

import type { SerializedError } from '@reduxjs/toolkit';

import st from './SettingsPage.module.scss';

function SettingsPage() {
    const [logoutInProgress, setLogoutInProgress] = useState(false);
    const [mintInProgress, setMintInProgress] = useState(false);
    const [mintStatus, setMintStatus] = useState<boolean | null>(null);
    const [mintError, setMintError] = useState<string | null>(null);
    const dispatch = useAppDispatch();
    const handleLogout = useCallback(async () => {
        setLogoutInProgress(true);
        await dispatch(logout());
    }, [dispatch]);
    const handleMint = useCallback(async () => {
        setMintInProgress(true);
        setMintError(null);
        try {
            await dispatch(mintDemoNFT()).unwrap();
            setMintStatus(true);
        } catch (e) {
            setMintStatus(false);
            setMintError((e as SerializedError).message || null);
        } finally {
            setMintInProgress(false);
        }
    }, [dispatch]);
    const mintStatusIcon =
        mintStatus !== null ? (mintStatus ? 'check2' : 'x-lg') : null;
    useEffect(() => {
        let timeout: number;
        if (mintStatus !== null) {
            timeout = window.setTimeout(() => setMintStatus(null), 3000);
        }
        return () => {
            if (timeout) {
                clearTimeout(timeout);
            }
        };
    }, [mintStatus]);
    return (
        <div className={st.container}>
            <div className={(st.item, st.network)}>
                <Network />
            </div>
            <div className={st.item}>
                <ExplorerLink
                    className="btn"
                    type={ExplorerLinkType.address}
                    useActiveAddress={true}
                >
                    View account on Sui Explorer
                </ExplorerLink>
            </div>
            <div className={st.item}>
                <ExternalLink className="btn" href={ToS_LINK}>
                    Terms of Service
                </ExternalLink>
            </div>
            <div className={st.item}>
                <button
                    type="button"
                    className={cl('btn', st['mint-btn'])}
                    onClick={handleMint}
                    disabled={mintInProgress || mintStatus !== null}
                >
                    {mintInProgress ? <LoadingIndicator /> : 'Mint Demo NFT'}
                    {mintStatusIcon ? (
                        <BsIcon
                            icon={mintStatusIcon}
                            className={cl(st['mint-icon'], {
                                [st.success]: mintStatus,
                                [st.fail]: !mintStatus,
                            })}
                        />
                    ) : null}
                </button>
                {mintError ? (
                    <Alert className={st['mint-error']}>
                        <strong>Minting demo NFT failed.</strong>
                        <small>{mintError}</small>
                    </Alert>
                ) : null}
            </div>
            <div className={st.item}>
                <button
                    type="button"
                    className="btn"
                    onClick={handleLogout}
                    disabled={logoutInProgress}
                >
                    {logoutInProgress ? <LoadingIndicator /> : 'Logout'}
                </button>
            </div>
        </div>
    );
}

export default SettingsPage;
