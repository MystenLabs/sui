// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useEffect, useState } from 'react';

import Footer from '../components/footer/Footer';
import Header from '../components/header/Header';
import { NetworkContext, useNetwork } from '../context';
import AppRoutes from '../pages/config/AppRoutes';

import styles from './App.module.css';

const createQueryClient = () =>
    new QueryClient({
        defaultOptions: {
            queries: {
                refetchOnMount: false,
                refetchOnWindowFocus: false,
            },
        },
    });

function App() {
    const [network, setNetwork] = useNetwork();
    const [queryClient] = useState(createQueryClient);

    // TODO: Verify this behavior:
    useEffect(() => {
        queryClient.clear();
    }, [network, queryClient]);

    return (
        <QueryClientProvider client={queryClient}>
            <NetworkContext.Provider value={[network, setNetwork]}>
                <div className={styles.app}>
                    <Header />
                    <main>
                        <section className={styles.suicontainer}>
                            <AppRoutes />
                        </section>
                    </main>
                    <Footer />
                </div>
            </NetworkContext.Provider>
        </QueryClientProvider>
    );
}

export default App;
