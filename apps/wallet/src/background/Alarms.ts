// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';

export const autoLockAlarmName = 'auto-lock-alarm';
export const cleanUpAlarmName = 'clean-up-storage-alarm';

class Alarms {
	public async setAutoLockAlarm(minutes: number) {
		Browser.alarms.create(autoLockAlarmName, { delayInMinutes: minutes });
	}

	public clearAutoLockAlarm() {
		return Browser.alarms.clear(autoLockAlarmName);
	}

	public async setCleanUpAlarm() {
		await Browser.alarms.create(cleanUpAlarmName, { periodInMinutes: 60 * 6 }); //  every 6 hours
	}
}

const alarms = new Alarms();
export default alarms;
