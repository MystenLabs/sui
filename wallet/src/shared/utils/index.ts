// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

export function openInNewTab() {
    const url = Browser.runtime.getURL('ui.html');
    return Browser.tabs.create({ url });
}
