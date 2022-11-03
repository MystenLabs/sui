// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ReactQueryDevtools } from '@tanstack/react-query-devtools';
import React, { useEffect } from 'react';
import { Toaster } from 'react-hot-toast';

import Footer from '../components/footer/Footer';
import Header from '../components/header/Header';
import { NetworkContext, useNetwork } from '../context';
import AppRoutes from '../pages/config/AppRoutes';
import { growthbook, loadFeatures } from '../utils/growthbook';

import styles from './App.module.css';

import { usePrevious } from '~/hooks/usePrevious';

const queryClient = new QueryClient({
    defaultOptions: {
        queries: {
            // We default the stale time to 5 minutes, which is an arbitrary number selected to
            // strike the balance between stale data and cache hits.
            // Individual queries can override this value based on their caching needs.
            staleTime: 5 * 60 * 1000,
            refetchInterval: false,
            refetchIntervalInBackground: false,
        },
    },
});

// As a side-effect of this module loading, we start loading the features:
loadFeatures();

function App() {
    const [network, setNetwork] = useNetwork();
    const previousNetwork = usePrevious(network);

    useEffect(() => {
        if (network !== previousNetwork) {
            queryClient.cancelQueries();
            queryClient.clear();
        }
    }, [previousNetwork, network]);

    useEffect(() => {
        growthbook.setAttributes({
            network,
        });
    }, [network]);

    return (
        // NOTE: We set a top-level key here to force the entire react tree to be re-created when the network changes:
        <React.Fragment key={network}>
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
                                    className:
                                        '!bg-issue-light !text-issue-dark',
                                    iconTheme: {
                                        primary: 'var(--issue-light)',
                                        secondary: 'var(--issue-dark)',
                                    },
                                },
                            }}
                        />

                        <ReactQueryDevtools />
                    </NetworkContext.Provider>
                </QueryClientProvider>
            </GrowthBookProvider>
        </React.Fragment>
    );
}

export default App;
