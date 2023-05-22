// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RpcClientContext, useAppsBackend } from '@mysten/core';
import { WalletKitProvider } from '@mysten/wallet-kit';
import { useQuery } from '@tanstack/react-query';
import { ReactQueryDevtools } from '@tanstack/react-query-devtools';
import { Fragment, useMemo } from 'react';
import { Toaster } from 'react-hot-toast';
import { Outlet, ScrollRestoration } from 'react-router-dom';

import { usePageView } from '../../hooks/usePageView';
import Footer from '../footer/Footer';
import Header from '../header/Header';

import { NetworkContext, useNetwork } from '~/context';
import { Banner } from '~/ui/Banner';
import { DefaultRpcClient, Network } from '~/utils/api/DefaultRpcClient';

export function Layout() {
    const [network, setNetwork] = useNetwork();
    const jsonRpcProvider = useMemo(() => DefaultRpcClient(network), [network]);
    const { request } = useAppsBackend();
    const { data } = useQuery({
        queryKey: ['apps-backend', 'monitor-network'],
        queryFn: () =>
            request<{ degraded: boolean }>('monitor-network', {
                project: 'EXPLORER',
            }),
        // Keep cached for 2 minutes:
        staleTime: 2 * 60 * 1000,
        retry: false,
        enabled: network === Network.MAINNET,
    });

    usePageView();

    return (
        // NOTE: We set a top-level key here to force the entire react tree to be re-created when the network changes:
        <Fragment key={network}>
            <ScrollRestoration />
            <WalletKitProvider
                /*autoConnect={false}*/
                enableUnsafeBurner={import.meta.env.DEV}
            >
                <RpcClientContext.Provider value={jsonRpcProvider}>
                    <NetworkContext.Provider value={[network, setNetwork]}>
                        <div className="w-full">
                            <Header />
                            <main className="relative z-10 min-h-screen bg-offwhite">
                                <section className="mx-auto max-w-[1440px] px-5 py-10 lg:px-10 2xl:px-0">
                                    {network === Network.MAINNET &&
                                        data?.degraded && (
                                            <div className="pb-2.5">
                                                <Banner
                                                    variant="warning"
                                                    border
                                                    fullWidth
                                                >
                                                    We&rsquo;re sorry that the
                                                    explorer is running slower
                                                    than usual. We&rsquo;re
                                                    working to fix the issue and
                                                    appreciate your patience.
                                                </Banner>
                                            </div>
                                        )}
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
                </RpcClientContext.Provider>
            </WalletKitProvider>
        </Fragment>
    );
}
