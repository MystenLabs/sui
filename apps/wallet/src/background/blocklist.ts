// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { BlocklistStorage, BlocklistStorageKey } from 'suisecblocklist';
import { SuiSecBlocklist } from 'suisecblocklist';

import { getFromLocalStorage, setToLocalStorage } from './storage-utils';

const storage: BlocklistStorage = {
	async getItem<T>(key: BlocklistStorageKey) {
		return (await getFromLocalStorage<T>(key)) as T | undefined;
	},
	async setItem(key: BlocklistStorageKey, data: unknown) {
		return await setToLocalStorage(key, data);
	},
};

export const blocklist = new SuiSecBlocklist(storage);
export { Action } from 'suisecblocklist';
