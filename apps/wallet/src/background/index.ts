// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { lte } from 'semver';
import Browser from 'webextension-polyfill';

import { LOCK_ALARM_NAME } from './Alarms';
import Keyring from './Keyring';
import Permissions from './Permissions';
import { Connections } from './connections';
import { openInNewTab } from '_shared/utils';
import { v4 as randomUUID } from 'uuid';

Browser.runtime.onInstalled.addListener(async ({ reason, previousVersion }) => {
    if (reason === 'install') {
        openInNewTab();
    } else if (
        reason === 'update' &&
        previousVersion &&
        lte(previousVersion, '0.1.1')
    ) {
        // clear everything in the storage
        // mainly done to clear the mnemonic that was stored
        // as plain text
        await Browser.storage.local.clear();
    }
});
const portStats: ConstructorParameters<typeof Connections>['0'] = {
    lastConnection: null,
    lastDisconnection: null,
    lastMessage: null,
};
const connections = new Connections(portStats);

Permissions.permissionReply.subscribe((permission) => {
    if (permission) {
        connections.notifyForPermissionReply(permission);
    }
});

Keyring.on('lockedStatusUpdate', (isLocked: boolean) => {
    connections.notifyForLockedStatusUpdate(isLocked);
});

Browser.alarms.onAlarm.addListener((alarm) => {
    if (alarm.name === LOCK_ALARM_NAME) {
        Keyring.lock();
    }
});

const myName = randomUUID();
const lastRestart = Date.now();
const intl = {
    milliseconds: new Intl.NumberFormat('en', {
        style: 'unit',
        unit: 'millisecond',
        unitDisplay: 'short',
    }),
    second: new Intl.NumberFormat('en', {
        style: 'unit',
        unit: 'second',
        unitDisplay: 'short',
    }),
};

function formatMillis(millis: number) {
    let unit: keyof typeof intl = 'milliseconds';
    let divisor = 1;
    if (millis >= 1000) {
        unit = 'second';
        divisor = 1000;
    }
    return intl[unit].format(millis / divisor);
}

function log(data: Record<string, unknown>) {
    const now = Date.now();
    const { lastConnection, lastDisconnection, lastMessage } = portStats;
    const aliveFor = formatMillis(now - lastRestart);
    const sinceLastConnection = lastConnection
        ? formatMillis(now - lastConnection.timestamp)
        : null;
    const sinceLastDisconnection = lastDisconnection
        ? formatMillis(now - lastDisconnection.timestamp)
        : null;
    const sinceLastMessage = lastMessage
        ? formatMillis(now - lastMessage.timestamp)
        : null;

    fetch('http://localhost:3000/logs', {
        method: 'post',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
            aliveFor,
            sinceLastConnection,
            sinceLastDisconnection,
            sinceLastMessage,
            myName,
            timestamp: now,
            lastConnection,
            lastDisconnection,
            lastMessage,
            ...data,
        }),
    });
}

console.log('bgStarted', Date.now());
log({ msg: 'bgStarted' });
Browser.storage.local.set({
    'stats.bgStarted': Date.now(),
});

Browser.runtime.onSuspend.addListener(async () => {
    console.log('onSuspendCanceled', Date.now());
    log({ msg: 'onSuspendCanceled' });
    Browser.storage.local.set({
        'stats.bgOnSuspend': Date.now(),
    });
    console.log(
        await Browser.storage.local.get([
            'stats.bgStarted',
            'stats.lastUIStatusMsg',
        ])
    );
});

Browser.runtime.onSuspendCanceled.addListener(() => {
    console.log('onSuspendCanceled', Date.now());
    log({ msg: 'onSuspendCanceled' });
    Browser.storage.local.set({
        'stats.bgOnSuspendCanceled': Date.now(),
    });
});

setInterval(() => {
    let guess = Math.random();
    let count = 0;
    while (guess > 0.1) {
        count++;
        guess = Math.random();
    }
    console.log(`escaped after ${count} tries. Last guess (${guess})`);
    log({ msg: `escaped after ${count} tries. Last guess (${guess})` });
}, 1000);
