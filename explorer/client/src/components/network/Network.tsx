// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useContext, useState } from 'react';

import { ReactComponent as DownSVG } from '../../assets/Down.svg';
import { NetworkContext } from '../../context';
import { Network, getEndpoint } from '../../utils/api/DefaultRpcClient';
import {
    IS_STATIC_ENV,
    IS_LOCAL_ENV,
    IS_STAGING_ENV,
} from '../../utils/envUtil';

import styles from './Network.module.css';

export default function NetworkSelect() {
    const [network, setNetwork] = useContext(NetworkContext);
    const [isModuleOpen, setModuleOpen] = useState(false);
    const [isOpenInput, setIsOpenInput] = useState(false);

    const openModal = useCallback(
        () => (isModuleOpen ? setModuleOpen(false) : setModuleOpen(true)),
        [isModuleOpen, setModuleOpen]
    );
    const closeModal = useCallback(() => setModuleOpen(false), [setModuleOpen]);

    const openInput = useCallback(() => {
        setIsOpenInput(true);
        setNetwork(getEndpoint(Network.Local));
    }, [setIsOpenInput, setNetwork]);

    const chooseNetwork = useCallback(
        (specified: Network | string) => () => {
            if (network !== specified) {
                setNetwork(specified);
                setIsOpenInput(false);
            }
        },
        [network, setNetwork]
    );

    const handleTextChange = useCallback(
        (e: React.ChangeEvent<HTMLInputElement>) =>
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

    if (IS_LOCAL_ENV)
        return (
            <div>
                <div className={styles.networkbox}>Local</div>
            </div>
        );

    if (IS_STATIC_ENV)
        return (
            <div>
                <div className={styles.networkbox}>Static JSON</div>
            </div>
        );

    return (
        <div>
            <div onClick={openModal} className={styles.networkbox}>
                {network} <DownSVG />
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
                            onClick={chooseNetwork(Network.Devnet)}
                            className={networkStyle(Network.Devnet)}
                        >
                            Devnet
                        </div>
                        {IS_STAGING_ENV ? (
                            <div
                                onClick={chooseNetwork(Network.Staging)}
                                className={networkStyle(Network.Staging)}
                            >
                                Staging
                            </div>
                        ) : null}
                        <div
                            onClick={chooseNetwork(Network.Local)}
                            className={networkStyle(Network.Local)}
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
                                value={network}
                                onChange={handleTextChange}
                            />
                        )}
                    </div>
                </div>
                <div className={styles.detailsbg} onClick={closeModal}></div>
            </div>
        </div>
    );
}
