// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import '@fontsource/inter/variable.css';
import '@fontsource/red-hat-mono/variable.css';
import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { QueryClientProvider } from '@tanstack/react-query';
import { ReactQueryDevtools } from '@tanstack/react-query-devtools';
import { Analytics } from '@vercel/analytics/react';
import React from 'react';
import { Toaster } from 'react-hot-toast';

import Footer from '../components/footer/Footer';
import Header from '../components/header/Header';
import { NetworkContext, useNetwork } from '../context';
import AppRoutes from '../pages/config/AppRoutes';

import styles from './App.module.css';

import { growthbook, loadFeatures } from '~/utils/growthbook';
import { queryClient } from '~/utils/queryClient';

// As a side-effect of this module loading, we start loading the features:
loadFeatures();

function App() {
    const [network, setNetwork] = useNetwork();

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
                            containerStyle={{
                                top: 40,
                                left: 40,
                                bottom: 40,
                                right: 40,
                            }}
                            toastOptions={{
                                duration: 4000,
                                success: {
                                    icon: null,
                                    className:
                                        '!bg-success-light !text-success-dark',
                                },
                                error: {
                                    icon: null,
                                    className:
                                        '!bg-issue-light !text-issue-dark',
                                },
                            }}
                        />

                        <ReactQueryDevtools />
                        <Analytics />
                    </NetworkContext.Provider>
                </QueryClientProvider>
            </GrowthBookProvider>
        </React.Fragment>
    );
}

export default App;
