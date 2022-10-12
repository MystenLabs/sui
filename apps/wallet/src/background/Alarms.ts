// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import Keyring from './Keyring';
import {
    AUTO_LOCK_TIMER_DEFAULT_INTERVAL_MINUTES,
    AUTO_LOCK_TIMER_STORAGE_KEY,
} from '_src/shared/constants';

export const LOCK_ALARM_NAME = 'lock-keyring-alarm';

class Alarms {
    public async setLockAlarm() {
        if (Keyring.isLocked) {
            return;
        }
        const delayInMinutes = (
            await Browser.storage.local.get({
                [AUTO_LOCK_TIMER_STORAGE_KEY]:
                    AUTO_LOCK_TIMER_DEFAULT_INTERVAL_MINUTES,
            })
        )[AUTO_LOCK_TIMER_STORAGE_KEY];
        Browser.alarms.create(LOCK_ALARM_NAME, { delayInMinutes });
    }

    public clearAlarm(name: string) {
        return Browser.alarms.clear(name);
    }
}

export default new Alarms();
