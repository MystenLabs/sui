// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useCallback, useEffect, useState } from 'react';

import Alert from '_components/alert';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Icon from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import Layout from '_components/menu/content/layout';
import { useNextMenuUrl } from '_components/menu/hooks';
import { useAppDispatch } from '_hooks';
import { mintDemoNFT } from '_redux/slices/sui-objects';

import type { SerializedError } from '@reduxjs/toolkit';

import st from './Playground.module.scss';

function Playground() {
    const [mintInProgress, setMintInProgress] = useState(false);
    const [mintStatus, setMintStatus] = useState<boolean | null>(null);
    const [mintError, setMintError] = useState<string | null>(null);
    const dispatch = useAppDispatch();
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
    const mainMenuUrl = useNextMenuUrl(true, '/');
    return (
        <Layout backUrl={mainMenuUrl} title="Playground">
            <div className={st.container}>
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
                    <button
                        type="button"
                        className={cl('btn', st['mint-btn'])}
                        onClick={handleMint}
                        disabled={mintInProgress || mintStatus !== null}
                    >
                        {mintInProgress ? (
                            <LoadingIndicator />
                        ) : (
                            'Mint Demo NFT'
                        )}
                        {mintStatusIcon ? (
                            <Icon
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
            </div>
        </Layout>
    );
}

export default Playground;
