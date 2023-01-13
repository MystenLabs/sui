// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { lte, coerce } from 'semver';
import Browser from 'webextension-polyfill';

import { LOCK_ALARM_NAME } from './Alarms';
import Permissions from './Permissions';
import { Connections } from './connections';
import Keyring from './keyring';
import { IS_SESSION_STORAGE_SUPPORTED } from './keyring/VaultStorage';
import { openInNewTab } from '_shared/utils';
import { MSG_CONNECT } from '_src/content-script/keep-bg-alive';

Browser.runtime.onInstalled.addListener(async ({ reason, previousVersion }) => {
    // Skip automatically opening the onboarding in end-to-end tests.
    if (navigator.userAgent === 'Playwright') {
        return;
    }

    // TODO: Our versions don't use semver, and instead are date-based. Instead of using the semver
    // library, we can use some combination of parsing into a date + inspecting patch.
    const previousVersionSemver = coerce(previousVersion)?.version;

    if (reason === 'install') {
        openInNewTab();
    } else if (
        reason === 'update' &&
        previousVersionSemver &&
        lte(previousVersionSemver, '0.1.1')
    ) {
        // clear everything in the storage
        // mainly done to clear the mnemonic that was stored
        // as plain text
        await Browser.storage.local.clear();
    }
});

const connections = new Connections();

Permissions.permissionReply.subscribe((permission) => {
    if (permission) {
        connections.notifyForPermissionReply(permission);
    }
});

Permissions.on('connectedAccountsChanged', ({ origin, accounts }) => {
    connections.notifyWalletStatusChange(origin, { accounts });
});

Keyring.on('lockedStatusUpdate', (isLocked: boolean) => {
    connections.notifyForLockedStatusUpdate(isLocked);
});

Browser.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === LOCK_ALARM_NAME) {
        Keyring.reviveDone.finally(() => Keyring.lock());
    }
});

if (!IS_SESSION_STORAGE_SUPPORTED) {
    Keyring.on('lockedStatusUpdate', async (isLocked) => {
        if (!isLocked) {
            const allTabs = await Browser.tabs.query({});
            for (const aTab of allTabs) {
                if (aTab.id) {
                    try {
                        await Browser.tabs.sendMessage(aTab.id, MSG_CONNECT);
                    } catch (e) {
                        // not all tabs have the cs installed
                    }
                }
            }
        }
    });
}
