// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import Alarms, { LOCK_ALARM_NAME } from './Alarms';
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
    connections.notifyForLockedStatusUpdate(isLocked);
    if (isLocked) {
        Alarms.clearAlarm(LOCK_ALARM_NAME);
    } else if (connections.totalUiConnections === 0) {
        Alarms.setLockAlarm();
    }
});

connections.on('totalUiChanged', (ui) => {
    if (ui === 0) {
        Alarms.setLockAlarm();
    } else {
        Alarms.clearAlarm(LOCK_ALARM_NAME);
    }
});

Browser.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === LOCK_ALARM_NAME) {
        Keyring.lock();
    }
});
