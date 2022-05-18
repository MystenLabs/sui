// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createRoot } from 'react-dom/client';
import { IntlProvider } from 'react-intl';
import { Provider } from 'react-redux';
import { HashRouter } from 'react-router-dom';

import App from './app';
import { initAppType } from '_redux/slices/app';
import { getFromLocationSearch } from '_redux/slices/app/AppType';
import store from '_store';

import './styles/global.scss';

// TODO only in dev
(window as unknown as Record<string, unknown>)['store'] = store;

store.dispatch(initAppType(getFromLocationSearch(window.location.search)));

const rootDom = document.getElementById('root');
if (!rootDom) {
    throw new Error('Root element not found');
}
const root = createRoot(rootDom);
root.render(
    <HashRouter>
        <Provider store={store}>
            <IntlProvider locale={navigator.language}>
                <App />
            </IntlProvider>
        </Provider>
    </HashRouter>
);
