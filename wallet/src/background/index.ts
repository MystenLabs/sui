// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

Browser.runtime.onInstalled.addListener((details) => {
    if (details.reason === 'install') {
        const url = Browser.runtime.getURL('ui.html');
        Browser.tabs.create({ url });
    }
});
