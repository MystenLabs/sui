// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Footer from '../components/footer/Footer';
import Header from '../components/header/Header';
import { NetworkContext, useNetwork } from '../context';
import AppRoutes from '../pages/config/AppRoutes';

import styles from './App.module.css';

function App() {
    const [network, setNetwork] = useNetwork();
    return (
        <NetworkContext.Provider value={[network, setNetwork]}>
            <div className={styles.app}>
                <Header />
                <main>
                    <AppRoutes />
                </main>
                <Footer />
            </div>
        </NetworkContext.Provider>
    );
}

export default App;
