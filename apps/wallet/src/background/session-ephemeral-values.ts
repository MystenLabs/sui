// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Serializable } from '_shared/cryptography/keystore';
import {
	getEncryptedFromSessionStorage,
	removeFromSessionStorage,
	setToSessionStorageEncrypted,
} from '_src/background/storage-utils';

export function getEphemeralValue<T extends Serializable>(id: string) {
	return getEncryptedFromSessionStorage<T>(id);
}

export function setEphemeralValue<T extends Serializable>(id: string, data: T) {
	return setToSessionStorageEncrypted(id, data);
}

export function clearEphemeralValue(id: string) {
	return removeFromSessionStorage(id);
}
