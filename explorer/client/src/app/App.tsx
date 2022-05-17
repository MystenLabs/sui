// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { Link } from 'react-router-dom';

import Footer from '../components/footer/Footer';
import NetworkSelect from '../components/network/Network';
import Search from '../components/search/Search';
import { NetworkContext } from '../context';
import AppRoutes from '../pages/config/AppRoutes';
import { Network } from '../utils/api/DefaultRpcClient';

import styles from './App.module.css';

function App() {
    const [network, setNetwork] = useState<Network>(Network.Devnet);

    return (
        <NetworkContext.Provider value={[network, setNetwork]}>
            <div className={styles.app}>
                <div className={styles.search}>
                    <div className={styles.header}>
                        <Link className={styles.suititle} to="/">
                            Sui Explorer
                        </Link>
                        <NetworkSelect />
                    </div>
                    <Search />
                </div>
                <main>
                    <AppRoutes />
                </main>
                <Footer />
            </div>
        </NetworkContext.Provider>
    );
}

export default App;
