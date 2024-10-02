// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	decrypt,
	encrypt,
	getRandomPassword,
	makeEphemeraPassword,
	type Serializable,
} from '_src/shared/cryptography/keystore';
import { v4 as uuidV4 } from 'uuid';
import Browser from 'webextension-polyfill';
import type { Storage } from 'webextension-polyfill';

const SESSION_STORAGE: Storage.LocalStorageArea | null =
	// @ts-expect-error chrome
	global?.chrome?.storage?.session || null;

async function getFromStorage<T>(
	storage: Storage.LocalStorageArea,
	key: string,
	defaultValue: T | null = null,
): Promise<T | null> {
	return (await storage.get({ [key]: defaultValue }))[key];
}

async function setToStorage<T>(
	storage: Storage.LocalStorageArea,
	key: string,
	value: T,
): Promise<void> {
	return await storage.set({ [key]: value });
}

export function isSessionStorageSupported() {
	return !!SESSION_STORAGE;
}

//eslint-disable-next-line @typescript-eslint/no-explicit-any
type OmitFirst<T extends any[]> = T extends [any, ...infer R] ? R : never;
type GetParams<T> = OmitFirst<Parameters<typeof getFromStorage<T>>>;
type SetParams<T> = OmitFirst<Parameters<typeof setToStorage<T>>>;

export function getFromLocalStorage<T>(...params: GetParams<T>) {
	return getFromStorage<T>(Browser.storage.local, ...params);
}
export function setToLocalStorage<T>(...params: SetParams<T>) {
	return setToStorage<T>(Browser.storage.local, ...params);
}
export async function getFromSessionStorage<T>(...params: GetParams<T>) {
	if (!SESSION_STORAGE) {
		return null;
	}
	return getFromStorage<T>(SESSION_STORAGE, ...params);
}
export async function setToSessionStorage<T>(...params: SetParams<T>) {
	if (!SESSION_STORAGE) {
		return;
	}
	return setToStorage<T>(SESSION_STORAGE, ...params);
}
export async function removeFromSessionStorage(key: string) {
	if (!SESSION_STORAGE) {
		return;
	}
	await SESSION_STORAGE.remove(key);
}
export async function setToSessionStorageEncrypted<T extends Serializable>(key: string, value: T) {
	const random = getRandomPassword();
	await setToSessionStorage(key, {
		random,
		data: await encrypt(makeEphemeraPassword(random), value),
	});
}
export async function getEncryptedFromSessionStorage<T extends Serializable>(key: string) {
	const encryptedData = await getFromSessionStorage<{ random: string; data: string }>(key, null);
	if (!encryptedData) {
		return null;
	}
	try {
		return decrypt<T>(makeEphemeraPassword(encryptedData.random), encryptedData.data);
	} catch (e) {
		return null;
	}
}

/**
 * Generates a unique id using uuid, that can be used as a key for storage data
 */
export function makeUniqueKey() {
	return uuidV4();
}
