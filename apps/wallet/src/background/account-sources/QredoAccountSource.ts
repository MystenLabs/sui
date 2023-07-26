// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { getAccountSources } from '.';
import {
	AccountSource,
	type AccountSourceSerialized,
	type AccountSourceSerializedUI,
} from './AccountSource';
import { type QredoConnectIdentity } from '../qredo/types';
import { isSameQredoConnection } from '../qredo/utils';
import { setStorageEntity } from '../storage-entities-utils';
import { makeUniqueKey } from '../storage-utils';
import { decrypt, encrypt } from '_src/shared/cryptography/keystore';
import { QredoAPI } from '_src/shared/qredo-api';

type DataDecryptedV0 = {
	version: 0;
	refreshToken: string;
};

type EphemeralData = {
	refreshToken: string;
	accessToken: string;
};

interface QredoAccountSourceSerialized extends AccountSourceSerialized, QredoConnectIdentity {
	type: 'qredo';
	encrypted: string;
}

interface QredoAccountSourceSerializedUI extends AccountSourceSerializedUI, QredoConnectIdentity {
	type: 'qredo';
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
	}: {
		password: string;
		apiUrl: string;
		organization: string;
		origin: string;
		service: string;
		refreshToken: string;
	}) {
		const decryptedData: DataDecryptedV0 = {
			version: 0,
			refreshToken,
		};
		const dataSerialized: QredoAccountSourceSerialized = {
			id: makeUniqueKey(),
			storageEntityType: 'account-source-entity',
			type: 'qredo',
			apiUrl,
			organization,
			origin,
			service,
			encrypted: await encrypt(password, decryptedData),
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
		await setStorageEntity(dataSerialized);
		return new QredoAccountSource(dataSerialized.id);
	}

	static isOfType(serialized: AccountSourceSerialized): serialized is QredoAccountSourceSerialized {
		return serialized.type === 'qredo';
	}

	async toUISerialized(): Promise<QredoAccountSourceSerializedUI> {
		const { apiUrl, id, organization, origin, service } = await this.getStoredData();
		return {
			id,
			type: 'qredo',
			origin,
			apiUrl,
			organization,
			service,
			isLocked: await this.isLocked(),
		};
	}

	async isLocked(): Promise<boolean> {
		return !(await this.getEphemeralValue());
	}

	lock(): Promise<void> {
		return this.clearEphemeralValue();
	}

	async unlock(password: string) {
		const { encrypted } = await this.getStoredData();
		const { refreshToken } = await decrypt<DataDecryptedV0>(password, encrypted);
		return this.setEphemeralValue({
			refreshToken,
			accessToken: await this.#createAccessToken(refreshToken),
		});
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
