// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import Browser from 'webextension-polyfill';
import { getFromLocalStorage, setToLocalStorage } from './storage-utils';

const allStorageEntitiesTypes = ['account-source-entity', 'account-entity'] as const;
export type StorageEntityType = (typeof allStorageEntitiesTypes)[number];
export type UIAccessibleEntityType = Exclude<StorageEntityType, 'active-account-entity'>;
export interface StorageEntity {
	id: string;
	storageEntityType: StorageEntityType;
}

export function isStorageEntity(item: any): item is StorageEntity {
	return !!(
		item &&
		typeof item === 'object' &&
		'id' in item &&
		typeof item.id === 'string' &&
		item.id.length &&
		'storageEntityType' in item &&
		allStorageEntitiesTypes.includes(item.storageEntityType)
	);
}

export function getStorageEntity<R extends StorageEntity>(id: string) {
	return getFromLocalStorage<R>(id, null);
}

export function setStorageEntity<T extends StorageEntity>(entity: T) {
	return setToLocalStorage(entity.id, entity);
}

export async function updateStorageEntity<
	T extends StorageEntity,
	U extends Partial<Omit<T, 'id' | 'type'>> = {},
>(id: string, update: U) {
	const existingData = await getStorageEntity<T>(id);
	if (!existingData) {
		throw new Error(`Entity ${id} not found`);
	}
	return setStorageEntity({ ...existingData, ...update });
}

export function deleteStorageEntity(id: string) {
	return Browser.storage.local.remove(id);
}

export async function deleteAllStorageEntities(type: StorageEntityType) {
	const allKeys = Object.entries(await Browser.storage.local.get(null))
		.filter(([_, anItem]) => isStorageEntity(anItem) && type === anItem.storageEntityType)
		.map(([aKey]) => aKey);
	if (allKeys.length) {
		return Browser.storage.local.remove(allKeys);
	}
}

export async function getAllStoredEntities<R extends StorageEntity>(type: StorageEntityType) {
	return Object.values(await Browser.storage.local.get(null))
		.filter((anItem) => isStorageEntity(anItem) && anItem.storageEntityType === type)
		.map((anEntity) => anEntity as R);
}
