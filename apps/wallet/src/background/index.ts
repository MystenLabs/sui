// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import Keyring from './Keyring';
import Permissions from './Permissions';
import { Connections } from './connections';
import { openInNewTab } from '_shared/utils';

Browser.runtime.onInstalled.addListener((details) => {
    if (details.reason === 'install') {
        openInNewTab();
    }
});

const connections = new Connections();

Permissions.permissionReply.subscribe((permission) => {
    if (permission) {
        connections.notifyForPermissionReply(permission);
    }
});

Keyring.on('lockedStatusUpdate', (isLocked: boolean) => {
    // TODO notify UI
});
