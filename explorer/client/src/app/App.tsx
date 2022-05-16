// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';
import { Link } from 'react-router-dom';

import Footer from '../components/footer/Footer';
import Network from '../components/network/Network';
import Search from '../components/search/Search';
import { NetworkContext } from '../context';
import AppRoutes from '../pages/config/AppRoutes';

import styles from './App.module.css';

function App() {
    const [network, setNetwork] = useState('Devnet');

    return (
        <NetworkContext.Provider value={[network, setNetwork]}>
            <div className={styles.app}>
                <div className={styles.search}>
                    <h2 className={styles.suititle}>
                        <Link to="/">Sui Explorer</Link>
                    </h2>
                    <Search />
                    <Network />
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
