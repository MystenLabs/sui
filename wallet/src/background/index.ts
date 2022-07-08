// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import { Connections } from './connections';
import { openInNewTab } from '_shared/utils';

Browser.runtime.onInstalled.addListener((details) => {
    if (details.reason === 'install') {
        openInNewTab();
    }
});

new Connections();
