// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { Toaster } from 'react-hot-toast';

import Footer from '../components/footer/Footer';
import Header from '../components/header/Header';
import { NetworkContext, useNetwork } from '../context';
import AppRoutes from '../pages/config/AppRoutes';
import { growthbook, loadFeatures } from '../utils/growthbook';

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

// As a side-effect of this module loading, we start loading the features:
loadFeatures();

function App() {
    const [network, setNetwork] = useNetwork();
    const [queryClient] = useState(createQueryClient);

    // TODO: Verify this behavior:
    useEffect(() => {
        queryClient.clear();
    }, [network, queryClient]);

    return (
        <GrowthBookProvider growthbook={growthbook}>
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

                    <Toaster
                        position="bottom-center"
                        gutter={8}
                        toastOptions={{
                            duration: 4000,
                            success: {
                                className:
                                    '!bg-success-light !text-success-dark',
                                iconTheme: {
                                    primary: 'var(--success-light)',
                                    secondary: 'var(--success-dark)',
                                },
                            },
                            error: {
                                className: '!bg-issue-light !text-issue-dark',
                                iconTheme: {
                                    primary: 'var(--issue-light)',
                                    secondary: 'var(--issue-dark)',
                                },
                            },
                        }}
                    />
                </NetworkContext.Provider>
            </QueryClientProvider>
        </GrowthBookProvider>
    );
}

export default App;
