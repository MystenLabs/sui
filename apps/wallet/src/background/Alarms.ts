// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

import { getFromLocalStorage } from './storage-utils';
import {
	AUTO_LOCK_TIMER_DEFAULT_INTERVAL_MINUTES,
	AUTO_LOCK_TIMER_STORAGE_KEY,
} from '_src/shared/constants';

export const LOCK_ALARM_NAME = 'lock-keyring-alarm';

class Alarms {
	public async setLockAlarm() {
		const delayInMinutes =
			Number(await getFromLocalStorage<number | string>(AUTO_LOCK_TIMER_STORAGE_KEY)) ||
			AUTO_LOCK_TIMER_DEFAULT_INTERVAL_MINUTES;
		Browser.alarms.create(LOCK_ALARM_NAME, { delayInMinutes });
	}

	public clearLockAlarm() {
		return Browser.alarms.clear(LOCK_ALARM_NAME);
	}
}

export default new Alarms();
