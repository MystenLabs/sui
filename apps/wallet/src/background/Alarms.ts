// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import Keyring from './Keyring';

export const LOCK_ALARM_NAME = 'lock-keyring-alarm';

class Alarms {
    public async setLockAlarm() {
        if (Keyring.isLocked) {
            return;
        }
        const delayInMinutes = 5;
        Browser.alarms.create(LOCK_ALARM_NAME, { delayInMinutes });
    }

    public clearAlarm(name: string) {
        return Browser.alarms.clear(name);
    }
}

export default new Alarms();
