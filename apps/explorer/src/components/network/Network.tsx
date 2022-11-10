// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useFeature } from '@growthbook/growthbook-react';
import { useCallback, useContext, useState } from 'react';

import { ReactComponent as DownSVG } from '../../assets/Down.svg';
import { NetworkContext } from '../../context';
import { Network, getEndpoint } from '../../utils/api/DefaultRpcClient';
import { IS_STATIC_ENV, IS_STAGING_ENV } from '../../utils/envUtil';
import { GROWTHBOOK_FEATURES } from '../../utils/growthbook';

import styles from './Network.module.css';

const NETWORK_DISPLAY_NAME: Record<Network, string> = {
    [Network.LOCAL]: 'Local',
    [Network.DEVNET]: 'Devnet',
    [Network.STAGING]: 'Staging',
    [Network.TESTNET]: 'Testnet',
    [Network.STATIC]: 'Static JSON',
    [Network.CI]: 'CI',
};

export default function NetworkSelect() {
    const [network, setNetwork] = useContext(NetworkContext);
    const [isModuleOpen, setModuleOpen] = useState(false);
    const [isOpenInput, setIsOpenInput] = useState(() => !(network in Network));

    const showTestNet = useFeature(
        GROWTHBOOK_FEATURES.USE_TEST_NET_ENDPOINT
    ).on;

    const openModal = useCallback(
        () => (isModuleOpen ? setModuleOpen(false) : setModuleOpen(true)),
        [isModuleOpen, setModuleOpen]
    );

    const closeModal = useCallback(() => setModuleOpen(false), [setModuleOpen]);

    const openInput = useCallback(() => {
        setIsOpenInput(true);
        setNetwork(getEndpoint(Network.LOCAL));
    }, [setIsOpenInput, setNetwork]);

    const chooseNetwork = useCallback(
        (specified: Network | string) => () => {
            setNetwork(specified);
            setIsOpenInput(false);
        },
        [setNetwork]
    );

    const handleTextChange = useCallback(
        (e: React.FocusEvent<HTMLInputElement>) =>
            setNetwork(e.currentTarget.value),
        [setNetwork]
    );

    const networkStyle = (iconNetwork: Network | 'other') =>
        // Button text matches network or
        network === iconNetwork ||
        // network is not one of options and button text is other
        (iconNetwork === 'other' &&
            !Object.values(Network).includes(network as Network))
            ? styles.active
            : styles.inactive;

    if (IS_STATIC_ENV)
        return (
            <div>
                <div className={styles.networkbox}>Static JSON</div>
            </div>
        );

    const networkName =
        network in NETWORK_DISPLAY_NAME
            ? NETWORK_DISPLAY_NAME[network as Network]
            : network;

    return (
        <div>
            <div onClick={openModal} className={styles.networkbox}>
                {networkName} <DownSVG />
            </div>
            <div onClick={openModal} className={styles.hamburger}>
                <svg height="30.5" width="30.5">
                    <path d="M 2.5 10 H 28 M 2.5 18 H 28 M 2.5 26 H 28" />
                </svg>
            </div>
            <div
                className={isModuleOpen ? styles.opennetworkbox : styles.remove}
            >
                <div className={styles.opennetworkdetails}>
                    <div className={styles.closeicon} onClick={closeModal}>
                        &times;
                    </div>
                    <h2>Choose a Network</h2>
                    <div>
                        <div
                            onClick={chooseNetwork(Network.DEVNET)}
                            className={networkStyle(Network.DEVNET)}
                        >
                            Devnet
                        </div>
                        {IS_STAGING_ENV ? (
                            <div
                                onClick={chooseNetwork(Network.STAGING)}
                                className={networkStyle(Network.STAGING)}
                            >
                                Staging
                            </div>
                        ) : null}
                        {showTestNet ? (
                            <div
                                onClick={chooseNetwork(Network.TESTNET)}
                                className={networkStyle(Network.TESTNET)}
                            >
                                Testnet
                            </div>
                        ) : null}
                        <div
                            onClick={chooseNetwork(Network.LOCAL)}
                            className={networkStyle(Network.LOCAL)}
                        >
                            Local
                        </div>
                        <div
                            onClick={openInput}
                            className={networkStyle('other')}
                        >
                            Custom RPC URL
                        </div>
                        {isOpenInput && (
                            <input
                                type="text"
                                defaultValue={network}
                                onBlur={handleTextChange}
                            />
                        )}
                    </div>
                </div>
                <div className={styles.detailsbg} onClick={closeModal} />
            </div>
        </div>
    );
}
