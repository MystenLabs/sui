// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { type z } from 'zod';

import { type zodSchema } from '../components/accounts/AutoLockSelector';
import { useBackgroundClient } from './useBackgroundClient';

export type AutoLockInterval = z.infer<typeof zodSchema>['autoLock']['interval'];
export const autoLockMinutesQueryKey = ['get auto-lock minutes'];

export function useAutoLockMinutes() {
	const backgroundClient = useBackgroundClient();
	return useQuery({
		queryKey: autoLockMinutesQueryKey,
		queryFn: () => backgroundClient.getAutoLockMinutes(),
		refetchInterval: 15 * 1000,
		meta: {
			skipPersistedCache: true,
		},
	});
}

const minutesOneDay = 60 * 24;
const minutesOneHour = 60;

export function formatAutoLock(minutes: number | null) {
	const { enabled, timer, interval } = parseAutoLock(minutes);
	if (!enabled) {
		return '';
	}
	return `${timer} ${interval}${timer === 1 ? '' : 's'}`;
}

export function parseAutoLock(minutes: number | null) {
	let timer = minutes || 1;
	const enabled = !!minutes;
	let interval: AutoLockInterval = 'hour';
	if (enabled) {
		if (minutes % minutesOneDay === 0) {
			timer = Math.floor(minutes / minutesOneDay);
			interval = 'day';
		} else if (minutes % minutesOneHour === 0) {
			timer = Math.floor(minutes / minutesOneHour);
			interval = 'hour';
		} else {
			interval = 'minute';
		}
	}
	return {
		enabled,
		timer,
		interval,
	};
}

const intervalToMinutesMultiplier: Record<AutoLockInterval, number> = {
	minute: 1,
	hour: minutesOneHour,
	day: minutesOneDay,
};

export function autoLockDataToMinutes({
	enabled,
	timer,
	interval,
}: {
	enabled: boolean;
	timer: number;
	interval: AutoLockInterval;
}) {
	if (!enabled) {
		return null;
	}
	return intervalToMinutesMultiplier[interval] * (Number(timer) || 1);
}
