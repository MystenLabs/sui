// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiClient } from '@mysten/sui.js/client';
import {
	IntentScope,
	messageWithIntent,
	type SerializedSignature,
} from '@mysten/sui.js/cryptography';
import { toB64 } from '@mysten/sui.js/utils';
import mitt from 'mitt';

import { WalletSigner } from './WalletSigner';
import { type SerializedQredoAccount } from '_src/background/keyring/QredoAccount';
import { API_ENV, networkNames } from '_src/shared/api-env';
import {
	type TransactionInfoResponse,
	type NetworkType,
	type QredoAPI,
} from '_src/shared/qredo-api';

export class QredoActionIgnoredByUser extends Error {}

const POLLING_INTERVAL = 4000;
const MAX_POLLING_TIMES = Math.ceil((60 * 1000 * 10) / POLLING_INTERVAL);

function sleep() {
	return new Promise((r) => setTimeout(r, POLLING_INTERVAL));
}
export const API_ENV_TO_QREDO_NETWORK: Partial<Record<API_ENV, NetworkType>> = {
	[API_ENV.mainnet]: 'mainnet',
	[API_ENV.testNet]: 'testnet',
	[API_ENV.devNet]: 'devnet',
};
export class QredoSigner extends WalletSigner {
	#qredoAccount: SerializedQredoAccount;
	#qredoAPI: QredoAPI;
	#network: NetworkType | null;
	#apiEnv: API_ENV;

	constructor(
		client: SuiClient,
		account: SerializedQredoAccount,
		qredoAPI: QredoAPI,
		apiEnv: API_ENV,
	) {
		super(client);
		this.#qredoAccount = account;
		this.#qredoAPI = qredoAPI;
		this.#apiEnv = apiEnv;
		this.#network = API_ENV_TO_QREDO_NETWORK[apiEnv] || null;
	}

	async getAddress(): Promise<string> {
		return this.#qredoAccount.address;
	}

	async signData(data: Uint8Array, clientIdentifier?: string): Promise<SerializedSignature> {
		let txInfo = await this.#createQredoTransaction(data, false, clientIdentifier);
		try {
			txInfo = await this.#pollForQredoTransaction(
				txInfo,
				(currentTxInfo) =>
					!currentTxInfo.sig &&
					['created', 'authorized', 'pending', 'approved', 'queued', 'signed'].includes(
						currentTxInfo.status,
					),
				clientIdentifier,
			);
		} finally {
			if (clientIdentifier) {
				QredoEvents.emit('qredoActionDone', {
					clientIdentifier,
					qredoTransactionID: txInfo.txID,
				});
			}
		}
		if (['cancelled', 'expired', 'failed', 'rejected'].includes(txInfo.status)) {
			throw new Error(`Transaction ${txInfo.status}`);
		}
		if (!txInfo.sig) {
			throw new Error('Failed to retrieve signature');
		}
		return txInfo.sig;
	}

	signMessage: WalletSigner['signMessage'] = async (input, clientIdentifier) => {
		const signature = await this.signData(
			messageWithIntent(IntentScope.PersonalMessage, input.message),
			clientIdentifier,
		);
		return {
			messageBytes: toB64(input.message),
			signature,
		};
	};

	signTransactionBlock: WalletSigner['signTransactionBlock'] = async (input, clientIdentifier) => {
		const transactionBlockBytes = await this.prepareTransactionBlock(input.transactionBlock);
		const signature = await this.signData(
			messageWithIntent(IntentScope.TransactionData, transactionBlockBytes),
			clientIdentifier,
		);
		return {
			transactionBlockBytes: toB64(transactionBlockBytes),
			signature,
		};
	};

	signAndExecuteTransactionBlock: WalletSigner['signAndExecuteTransactionBlock'] = async (
		{ transactionBlock, options },
		clientIdentifier,
	) => {
		let txInfo = await this.#createQredoTransaction(
			messageWithIntent(
				IntentScope.TransactionData,
				await this.prepareTransactionBlock(transactionBlock),
			),
			true,
			clientIdentifier,
		);
		try {
			txInfo = await this.#pollForQredoTransaction(
				txInfo,
				(tx) =>
					[
						'pending',
						'created',
						'authorized',
						'approved',
						'signed',
						'scheduled',
						'queued',
						'pushed',
					].includes(tx.status),
				clientIdentifier,
			);
		} finally {
			if (clientIdentifier) {
				QredoEvents.emit('qredoActionDone', {
					clientIdentifier,
					qredoTransactionID: txInfo.txID,
				});
			}
		}
		if (txInfo.status !== 'confirmed') {
			throw new Error(`Qredo transaction was ${txInfo.status}`);
		}
		if (!txInfo.txHash) {
			throw new Error(`Digest is not set in Qredo transaction ${txInfo.txID}`);
		}
		return this.provider.waitForTransactionBlock({
			digest: txInfo.txHash,
			options: options,
		});
	};

	connect(client: SuiClient): WalletSigner {
		return new QredoSigner(client, this.#qredoAccount, this.#qredoAPI, this.#apiEnv);
	}

	async #createQredoTransaction(intent: Uint8Array, broadcast: boolean, clientIdentifier?: string) {
		if (!this.#network) {
			throw new Error(`Unsupported network ${networkNames[this.#apiEnv]}`);
		}
		const qredoTransaction = await this.#qredoAPI.createTransaction({
			messageWithIntent: toB64(intent),
			network: this.#network,
			broadcast,
			from: await this.getAddress(),
		});
		if (clientIdentifier) {
			QredoEvents.emit('qredoTransactionCreated', {
				qredoTransaction,
				clientIdentifier,
			});
		}
		return qredoTransaction;
	}

	async #pollForQredoTransaction(
		qredoTransaction: TransactionInfoResponse,
		keepPolling: (qredoTransaction: TransactionInfoResponse) => boolean,
		clientIdentifier?: string,
	) {
		let unsubscribeCallback;
		let userIgnoredUpdates = false;
		let promiseReject: () => void;
		const userIgnoredPromise = new Promise<never>((_, reject) => {
			promiseReject = reject;
		});
		if (clientIdentifier) {
			unsubscribeCallback = (event: QredoEventsType['clientIgnoredUpdates']) => {
				if (clientIdentifier === event.clientIdentifier) {
					userIgnoredUpdates = true;
					if (promiseReject) {
						promiseReject();
					}
				}
			};
			QredoEvents.on('clientIgnoredUpdates', unsubscribeCallback);
		}
		let currentQredoTransaction = qredoTransaction;
		let requestCounter = 0;
		while (
			!userIgnoredUpdates &&
			keepPolling(currentQredoTransaction) &&
			requestCounter++ < MAX_POLLING_TIMES
		) {
			try {
				await Promise.race([sleep(), userIgnoredPromise]);
				currentQredoTransaction = await Promise.race([
					this.#qredoAPI.getTransaction(currentQredoTransaction.txID),
					userIgnoredPromise,
				]);
			} catch (e) {
				// maybe a network error
				// TODO: stop if 500 or 404 etc?
			}
		}
		if (clientIdentifier && unsubscribeCallback) {
			QredoEvents.off('clientIgnoredUpdates', unsubscribeCallback);
		}
		if (userIgnoredUpdates) {
			throw new QredoActionIgnoredByUser('User ignored transaction updates');
		}
		return currentQredoTransaction;
	}
}

export type QredoEventsType = {
	qredoTransactionCreated: {
		qredoTransaction: TransactionInfoResponse;
		clientIdentifier: string;
	};
	qredoActionDone: {
		clientIdentifier: string;
		qredoTransactionID: string;
	};
	clientIgnoredUpdates: {
		clientIdentifier: string;
	};
};
export const QredoEvents = mitt<QredoEventsType>();
