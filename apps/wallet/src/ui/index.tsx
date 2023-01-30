// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { QueryClientProvider } from '@tanstack/react-query';
import { createRoot } from 'react-dom/client';
import { IntlProvider } from 'react-intl';
import { Provider } from 'react-redux';
import { HashRouter } from 'react-router-dom';

import App from './app';
import { growthbook } from './app/experimentation/feature-gating';
import { queryClient } from './app/helpers/queryClient';
import { ErrorBoundary } from '_components/error-boundary';
import { initAppType } from '_redux/slices/app';
import { getFromLocationSearch } from '_redux/slices/app/AppType';
import { setAttributes } from '_src/shared/experimentation/features';
import initSentry from '_src/shared/sentry';
import store from '_store';
import { thunkExtras } from '_store/thunk-extras';

import './styles/global.scss';
import '@fontsource/inter/variable.css';
import '@fontsource/red-hat-mono/variable.css';
import '_font-icons/output/sui-icons.scss';
import 'bootstrap-icons/font/bootstrap-icons.scss';

async function init() {
    if (process.env.NODE_ENV === 'development') {
        Object.defineProperty(window, 'store', { value: store });
    }
    store.dispatch(initAppType(getFromLocationSearch(window.location.search)));
    await thunkExtras.background.init(store.dispatch);
    const { apiEnv, customRPC } = store.getState().app;
    setAttributes(growthbook, { apiEnv, customRPC });
}

function renderApp() {
    const rootDom = document.getElementById('root');
    if (!rootDom) {
        throw new Error('Root element not found');
    }
    const root = createRoot(rootDom);
    root.render(
        <GrowthBookProvider growthbook={growthbook}>
            <HashRouter>
                <Provider store={store}>
                    <IntlProvider locale={navigator.language}>
                        <QueryClientProvider client={queryClient}>
                            <ErrorBoundary>
                                <App />
                            </ErrorBoundary>
                        </QueryClientProvider>
                    </IntlProvider>
                </Provider>
            </HashRouter>
        </GrowthBookProvider>
    );
}

(async () => {
    await init();
    initSentry();
    renderApp();
})();
