// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { throttle } from 'throttle-debounce';

import Alarms from './Alarms';
import { getDB, settingsKeys } from './db';

export async function getAutoLockMinutes() {
	const minutesStored =
		(await (await getDB()).settings.get(settingsKeys.autoLockMinutes))?.value || null;
	return typeof minutesStored === 'number' ? minutesStored : null;
}

/**
 * minutes as null disables auto-lock
 * updates the alarm to the new timeout
 */
export async function setAutoLockMinutes(minutes: number | null) {
	await (
		await getDB()
	).settings.put({
		setting: settingsKeys.autoLockMinutes,
		value: minutes,
	});
	await setupAutoLockAlarm();
}

export async function setupAutoLockAlarm() {
	const minutes = await getAutoLockMinutes();
	if (!minutes) {
		Alarms.clearAutoLockAlarm();
	} else {
		Alarms.setAutoLockAlarm(minutes);
	}
}

export const notifyUserActive = throttle(
	5000,
	() => {
		setupAutoLockAlarm();
	},
	{ noTrailing: true },
);
