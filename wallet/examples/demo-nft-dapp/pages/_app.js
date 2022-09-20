// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectExistsResponse, JsonRpcProvider } from '@mysten/sui.js';
import { APIContext } from '../shared/api-context';
import Head from 'next/head';
import { useCallback, useEffect, useState, useMemo, useRef } from 'react';

import { useSuiWallet } from '../shared/useSuiWallet';

import styles from '../styles/Home.module.css';
import '../styles/globals.css';
import { AccountContext } from '../shared/account-context';
import { ObjectsStoreContext } from '../shared/objects-store-context';
import Link from 'next/link';
import { MODULE, PACKAGE_ID } from '../lottery/constants';

const OBJECTS_POLL_INTERVAL = 2 * 1e3;

// eslint-disable-next-line react/prop-types
function MyApp({ Component, pageProps }) {
    const api = useMemo(
        () => new JsonRpcProvider('https://fullnode.devnet.sui.io/'),
        []
    );
    const [walletInstalled, setWalletInstalled] = useState(null);
    const [connected, setConnected] = useState(false);
    const [connecting, setConnecting] = useState(false);
    const [msgNotice, setMsgNotice] = useState(null);
    const [account, setAccount] = useState(null);
    const [suiObjects, setSuiObjects] = useState({});
    const [suiObjectsTriggerCounter, setTriggerCounter] = useState(0);
    const suiWallet = useSuiWallet();
    useEffect(() => {
        setWalletInstalled(suiWallet && true);
        if (suiWallet) {
            suiWallet.hasPermissions().then(setConnected, setMsgNotice);
        }
    }, [suiWallet]);
    const onConnectClick = useCallback(async () => {
        if (!suiWallet) {
            return;
        }
        setConnecting(true);
        try {
            await suiWallet.requestPermissions();
            setConnected(true);
        } catch (e) {
            setMsgNotice(e);
        } finally {
            setConnecting(false);
        }
    }, [suiWallet]);
    useEffect(() => {
        if (connected && suiWallet) {
            suiWallet
                .getAccounts()
                .then((accounts) => setAccount(accounts[0]), setMsgNotice);
        } else {
            setAccount(null);
        }
    }, [connected, suiWallet]);
    useEffect(() => {
        let timeout;
        if (msgNotice) {
            timeout = setTimeout(() => setMsgNotice(null), 10000);
        }
        return () => clearTimeout(timeout);
    }, [msgNotice]);
    const prevData = useRef();
    useEffect(() => {
        let timeout;
        const load = async () => {
            if (account) {
                try {
                    const objRefs = await api.getObjectsOwnedByAddress(account);
                    const allObjIDs = objRefs.map((objRef) => objRef.objectId);
                    const objRes = (
                        allObjIDs.length
                            ? await api.getObjectBatch(allObjIDs)
                            : []
                    )
                        .map((obj) => getObjectExistsResponse(obj))
                        .filter(Boolean)
                        .sort((a, b) =>
                            a.reference.objectId.localeCompare(
                                b.reference.objectId
                            )
                        );
                    const allSuiObjects = {};
                    for (const suiObj of objRes) {
                        allSuiObjects[suiObj.reference.objectId] = suiObj;
                    }
                    const dataJSON = JSON.stringify(allSuiObjects);
                    if (!prevData.current || prevData.current !== dataJSON) {
                        console.log(allSuiObjects);
                        setSuiObjects(allSuiObjects);
                    }
                    prevData.current = dataJSON;
                } catch (e) {
                    console.error(e);
                }
                timeout = setTimeout(load, OBJECTS_POLL_INTERVAL);
            }
        };
        load();
        return () => {
            if (timeout) {
                clearInterval(timeout);
            }
        };
    }, [account, api, suiObjectsTriggerCounter]);
    const triggerUpdate = useCallback(() => {
        setTriggerCounter((c) => c + 1);
    }, []);
    const clear = useCallback(() => {
        setSuiObjects({});
    }, []);
    useEffect(() => {
        let timeout;
        const load = async () => {
            try {
                const events = await api.getEventsByModule(PACKAGE_ID, MODULE);
                console.log(events);
                // TODO:
                timeout = setTimeout(load, OBJECTS_POLL_INTERVAL);
            } catch (e) {
                console.error(e);
            }
        };
        load();
        return () => {
            if (timeout) {
                clearInterval(timeout);
            }
        };
    }, [api]);
    return (
        <div className={styles.container}>
            <Head>
                <link rel="icon" href="/favicon.png" />
            </Head>
            <Link href="/">🏠</Link>
            <main className={styles.main}>
                {walletInstalled ? (
                    <>
                        {connected ? (
                            <APIContext.Provider value={api}>
                                <AccountContext.Provider value={account}>
                                    <ObjectsStoreContext.Provider
                                        value={{
                                            triggerUpdate,
                                            suiObjects,
                                            clear,
                                        }}
                                    >
                                        <Component {...pageProps} />
                                    </ObjectsStoreContext.Provider>
                                </AccountContext.Provider>
                            </APIContext.Provider>
                        ) : (
                            <button
                                type="button"
                                onClick={onConnectClick}
                                disabled={connecting}
                            >
                                Connect
                            </button>
                        )}
                    </>
                ) : walletInstalled === false ? (
                    <h6>It seems Sui Wallet is not installed.</h6>
                ) : null}
                {msgNotice ? (
                    <div className="error">
                        <pre>
                            {msgNotice.message ||
                                JSON.stringify(msgNotice, null, 4)}
                        </pre>
                    </div>
                ) : null}
            </main>
        </div>
    );
}

export default MyApp;
