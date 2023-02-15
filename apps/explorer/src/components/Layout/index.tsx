// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { WalletKitProvider } from '@mysten/wallet-kit';
import { QueryClientProvider } from '@tanstack/react-query';
import { ReactQueryDevtools } from '@tanstack/react-query-devtools';
import { Fragment } from 'react';
import { Toaster } from 'react-hot-toast';
import { Outlet } from 'react-router-dom';

import { usePageView } from '../../hooks/usePageView';
import Footer from '../footer/Footer';
import Header from '../header/Header';

import { NetworkContext, useNetwork } from '~/context';
import { growthbook } from '~/utils/growthbook';
import { queryClient } from '~/utils/queryClient';

export function Layout() {
    const [network, setNetwork] = useNetwork();
    usePageView();
    return (
        // NOTE: We set a top-level key here to force the entire react tree to be re-created when the network changes:
        <Fragment key={network}>
            <WalletKitProvider
                /*autoConnect={false}*/
                enableUnsafeBurner={import.meta.env.DEV}
            >
                <GrowthBookProvider growthbook={growthbook}>
                    <QueryClientProvider client={queryClient}>
                        <NetworkContext.Provider value={[network, setNetwork]}>
                            <div className="w-full">
                                <Header />
                                <main className="relative z-10 min-h-screen bg-offwhite py-2">
                                    <section className="mx-auto max-w-[1440px] py-10 px-5 2xl:px-0">
                                        <Outlet />
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
                        </NetworkContext.Provider>
                    </QueryClientProvider>
                </GrowthBookProvider>
            </WalletKitProvider>
        </Fragment>
    );
}
