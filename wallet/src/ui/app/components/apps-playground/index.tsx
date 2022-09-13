// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { useCallback, useEffect, useState } from 'react';

import SuiApp from './SuiApp';
import Button from '_app/shared/button';
import ExplorerLink from '_components/explorer-link';
import { ExplorerLinkType } from '_components/explorer-link/ExplorerLinkType';
import Icon, { SuiIcons } from '_components/icon';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import { useAppDispatch } from '_hooks';
import { mintDemoNFT } from '_redux/slices/sui-objects';

import type { SerializedError } from '@reduxjs/toolkit';

import st from './Playground.module.scss';

function AppsPlayGround() {
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

    const appData = {
        title: 'BlueMove',
        description:
            "Discover the most outstanding NFTs in all topics of life. Buy NFTs (or sell 'em) to earn rewards.",
        icon: 'https://storage.googleapis.com/sui-cms-content/Gaggle_circle_gradient_a5d1a37283/Gaggle_circle_gradient_a5d1a37283.png',
        link: 'https://sui-wallet-demo.s3.amazonaws.com/index.html',
        tags: ['NFT Marketplace'],
    };

    return (
        <div className={cl(st.container)}>
            <div className={st.groupButtons}>
                <Button
                    size="large"
                    mode="outline"
                    className={cl('btn', st.cta, st['mint-btn'])}
                    onClick={handleMint}
                    disabled={mintInProgress || mintStatus !== null}
                >
                    {mintInProgress ? <LoadingIndicator /> : ' Mint an NFT'}

                    {mintStatusIcon ? (
                        <Icon
                            icon={mintStatusIcon}
                            className={cl(st['mint-icon'], {
                                [st.success]: mintStatus,
                                [st.fail]: !mintStatus,
                            })}
                        />
                    ) : (
                        <Icon
                            icon={SuiIcons.ArrowRight}
                            className={cl(st.arrowActionIcon, st.angledArrow)}
                        />
                    )}
                </Button>

                <ExplorerLink
                    className={cl('btn', st.cta, st.outline)}
                    type={ExplorerLinkType.address}
                    useActiveAddress={true}
                    showIcon={false}
                >
                    View account on Sui Explorer
                    <Icon
                        icon={SuiIcons.ArrowRight}
                        className={cl(st.arrowActionIcon, st.angledArrow)}
                    />
                </ExplorerLink>
                {mintError ? (
                    <div className={st.error}>
                        <strong>Minting NFT failed.</strong>
                        <div>
                            <small>{mintError}</small>
                        </div>
                    </div>
                ) : null}
            </div>
            <div className={st.desc}>
                <div className={st.title}>Builders in sui ecosystem</div>
                Apps here are actively curated but do not indicate any
                endorsement or relationship with Sui Wallet. Please DYOR.
            </div>

            <div className={st.apps}>
                <SuiApp {...appData} displaytype="full" />
                <SuiApp {...appData} displaytype="full" />
                <SuiApp {...appData} displaytype="full" />
                <SuiApp {...appData} displaytype="full" />
                <SuiApp {...appData} displaytype="full" />
            </div>
        </div>
    );
}

export default AppsPlayGround;
