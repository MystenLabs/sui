// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';
import Browser from 'webextension-polyfill';

import {
	AUTO_LOCK_TIMER_DEFAULT_INTERVAL_MINUTES,
	AUTO_LOCK_TIMER_STORAGE_KEY,
} from '_src/shared/constants';

export function useAutoLockInterval() {
	const [interval, setInterval] = useState<number | null>(null);
	useEffect(() => {
		Browser.storage.local
			.get({
				[AUTO_LOCK_TIMER_STORAGE_KEY]: AUTO_LOCK_TIMER_DEFAULT_INTERVAL_MINUTES,
			})
			.then(({ [AUTO_LOCK_TIMER_STORAGE_KEY]: storedTimer }) => setInterval(Number(storedTimer)));
		const changesCallback = (changes: Browser.Storage.StorageAreaOnChangedChangesType) => {
			if (AUTO_LOCK_TIMER_STORAGE_KEY in changes) {
				setInterval(
					Number(
						(changes[AUTO_LOCK_TIMER_STORAGE_KEY] as Browser.Storage.StorageChange).newValue,
					) || AUTO_LOCK_TIMER_DEFAULT_INTERVAL_MINUTES,
				);
			}
		};
		Browser.storage.local.onChanged.addListener(changesCallback);
		return () => {
			Browser.storage.local.onChanged.removeListener(changesCallback);
		};
	}, []);
	return interval;
}
