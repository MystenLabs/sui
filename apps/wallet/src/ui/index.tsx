// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { RpcClientContext } from '@mysten/core';
import { PersistQueryClientProvider } from '@tanstack/react-query-persist-client';
import { Fragment, StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { Provider } from 'react-redux';
import { HashRouter } from 'react-router-dom';

import App from './app';
import { SuiLedgerClientProvider } from './app/components/ledger/SuiLedgerClientProvider';
import { growthbook } from './app/experimentation/feature-gating';
import { persister, queryClient } from './app/helpers/queryClient';
import { useAppSelector } from './app/hooks';
import { ErrorBoundary } from '_components/error-boundary';
import { initAppType } from '_redux/slices/app';
import { getFromLocationSearch } from '_redux/slices/app/AppType';
import { setAttributes } from '_src/shared/experimentation/features';
import initSentry from '_src/shared/sentry';
import store from '_store';
import { api, thunkExtras } from '_store/thunk-extras';

import './styles/global.scss';
import '@fontsource/inter/variable.css';
import '@fontsource/red-hat-mono/variable.css';
import 'bootstrap-icons/font/bootstrap-icons.scss';

async function init() {
    if (process.env.NODE_ENV === 'development') {
        Object.defineProperty(window, 'store', { value: store });
    }
    store.dispatch(initAppType(getFromLocationSearch(window.location.search)));
    await thunkExtras.background.init(store.dispatch);
    const { apiEnv, customRPC } = store.getState().app;
    setAttributes({ apiEnv, customRPC });
}

function renderApp() {
    const rootDom = document.getElementById('root');
    if (!rootDom) {
        throw new Error('Root element not found');
    }
    const root = createRoot(rootDom);
    root.render(
        <StrictMode>
            <Provider store={store}>
                <AppWrapper />
            </Provider>
        </StrictMode>
    );
}

function AppWrapper() {
    const network = useAppSelector(
        ({ app: { apiEnv, customRPC } }) => `${apiEnv}_${customRPC}`
    );

    return (
        <GrowthBookProvider growthbook={growthbook}>
            <HashRouter>
                <SuiLedgerClientProvider>
                    {/*
                     * NOTE: We set a key here to force the entire react tree to be re-created when the network changes so that
                     * the RPC client instance (api.instance.fullNode) is updated correctly. In the future, we should look into
                     * making the API provider instance a reactive value and moving it out of the redux-thunk middleware
                     */}
                    <Fragment key={network}>
                        <PersistQueryClientProvider
                            client={queryClient}
                            persistOptions={{ persister }}
                        >
                            <RpcClientContext.Provider
                                value={api.instance.fullNode}
                            >
                                <ErrorBoundary>
                                    <App />
                                </ErrorBoundary>
                            </RpcClientContext.Provider>
                        </PersistQueryClientProvider>
                    </Fragment>
                </SuiLedgerClientProvider>
            </HashRouter>
        </GrowthBookProvider>
    );
}

(async () => {
    await init();
    initSentry();
    renderApp();
})();
