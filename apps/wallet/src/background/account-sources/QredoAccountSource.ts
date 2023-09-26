// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { decrypt, encrypt } from '_src/shared/cryptography/keystore';
import { QredoAPI } from '_src/shared/qredo-api';
import Dexie from 'dexie';

import { getAccountSources } from '.';
import { setupAutoLockAlarm } from '../auto-lock-accounts';
import { backupDB, getDB } from '../db';
import { type QredoConnectIdentity } from '../qredo/types';
import { isSameQredoConnection } from '../qredo/utils';
import { makeUniqueKey } from '../storage-utils';
import {
	AccountSource,
	type AccountSourceSerialized,
	type AccountSourceSerializedUI,
} from './AccountSource';
import { accountSourcesEvents } from './events';

type DataDecrypted = {
	refreshToken: string;
};

type EphemeralData = {
	refreshToken: string;
	accessToken: string;
};

interface QredoAccountSourceSerialized extends AccountSourceSerialized, QredoConnectIdentity {
	type: 'qredo';
	encrypted: string;
	originFavIcon: string;
}

interface QredoAccountSourceSerializedUI extends AccountSourceSerializedUI, QredoConnectIdentity {
	type: 'qredo';
	originFavIcon: string;
}

export class QredoAccountSource extends AccountSource<QredoAccountSourceSerialized, EphemeralData> {
	constructor(id: string) {
		super({ type: 'qredo', id });
	}
	static async createNew({
		password,
		apiUrl,
		organization,
		origin,
		service,
		refreshToken,
		originFavIcon,
	}: {
		password: string;
		apiUrl: string;
		organization: string;
		origin: string;
		service: string;
		refreshToken: string;
		originFavIcon: string;
	}) {
		const decryptedData: DataDecrypted = {
			refreshToken,
		};
		const dataSerialized: QredoAccountSourceSerialized = {
			id: makeUniqueKey(),
			type: 'qredo',
			apiUrl,
			organization,
			origin,
			service,
			encrypted: await encrypt(password, decryptedData),
			originFavIcon,
			createdAt: Date.now(),
		};
		const allAccountSources = await getAccountSources();
		for (const anAccountSource of allAccountSources) {
			if (
				anAccountSource instanceof QredoAccountSource &&
				isSameQredoConnection(
					{
						apiUrl: await anAccountSource.apiUrl,
						organization: await anAccountSource.organization,
						origin: await anAccountSource.origin,
						service: await anAccountSource.service,
					},
					{ id: dataSerialized.id, apiUrl, organization, origin, service },
				)
			) {
				throw new Error('Qredo account source already exists');
			}
		}
		return dataSerialized;
	}

	static isOfType(serialized: AccountSourceSerialized): serialized is QredoAccountSourceSerialized {
		return serialized.type === 'qredo';
	}

	static async save(
		serialized: QredoAccountSourceSerialized,
		{
			skipBackup = false,
			skipEventEmit = false,
		}: { skipBackup?: boolean; skipEventEmit?: boolean } = {},
	) {
		await (await Dexie.waitFor(getDB())).accountSources.put(serialized);
		if (!skipBackup) {
			await backupDB();
		}
		if (!skipEventEmit) {
			accountSourcesEvents.emit('accountSourcesChanged');
		}
		return new QredoAccountSource(serialized.id);
	}

	async toUISerialized(): Promise<QredoAccountSourceSerializedUI> {
		const { apiUrl, id, organization, origin, service, originFavIcon } = await this.getStoredData();
		return {
			id,
			type: 'qredo',
			origin,
			apiUrl,
			organization,
			service,
			isLocked: await this.isLocked(),
			originFavIcon,
		};
	}

	async isLocked(): Promise<boolean> {
		return !(await this.getEphemeralValue());
	}

	async lock(): Promise<void> {
		await this.clearEphemeralValue();
		accountSourcesEvents.emit('accountSourceStatusUpdated', { accountSourceID: this.id });
	}

	async unlock(password: string) {
		const { encrypted } = await this.getStoredData();
		const { refreshToken } = await decrypt<DataDecrypted>(password, encrypted);
		await this.setEphemeralValue({
			refreshToken,
			accessToken: await this.#createAccessToken(refreshToken),
		});
		await setupAutoLockAlarm();
		accountSourcesEvents.emit('accountSourceStatusUpdated', { accountSourceID: this.id });
	}

	async verifyPassword(password: string) {
		const { encrypted } = await this.getStoredData();
		await decrypt<DataDecrypted>(password, encrypted);
	}

	async renewAccessToken() {
		const ephemeralData = await this.getEphemeralValue();
		if (!ephemeralData) {
			throw new Error(`Qredo account source ${this.id} is locked`);
		}
		const { refreshToken } = ephemeralData;
		const accessToken = await this.#createAccessToken(refreshToken);
		await this.setEphemeralValue({ refreshToken, accessToken });
		return accessToken;
	}

	async #createAccessToken(refreshToken: string): Promise<string> {
		const { apiUrl } = await this.getStoredData();
		return (
			await new QredoAPI(this.id, apiUrl).createAccessToken({
				refreshToken,
			})
		).access_token;
	}

	get apiUrl() {
		return this.getStoredData().then(({ apiUrl }) => apiUrl);
	}

	get service() {
		return this.getStoredData().then(({ service }) => service);
	}

	get organization() {
		return this.getStoredData().then(({ organization }) => organization);
	}

	get origin() {
		return this.getStoredData().then(({ origin }) => origin);
	}

	get refreshToken() {
		return this.getEphemeralValue().then((data) => {
			if (!data) {
				throw new Error(`Qredo account source ${this.id} is locked`);
			}
			return data.refreshToken;
		});
	}

	get accessToken() {
		return this.getEphemeralValue().then((data) => {
			if (!data) {
				throw new Error(`Qredo account source ${this.id} is locked`);
			}
			return data.accessToken;
		});
	}
}
