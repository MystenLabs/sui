// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { toSearchQueryString } from './utils';

export type QredoAPIErrorResponse = {
	code: string;
	msg: string;
	detail: {
		reason: string;
	};
};

export class QredoAPIError extends Error {
	status: number;
	apiData: QredoAPIErrorResponse;

	constructor(status: number, apiData: QredoAPIErrorResponse) {
		super(`Qredo API Error (status: ${status}). ${apiData.msg}`);
		this.status = status;
		this.apiData = apiData;
	}
}

export class QredoAPIUnauthorizedError extends QredoAPIError {}

export type AccessTokenParams = {
	refreshToken: string;
	grantType?: string;
};

export type AccessTokenResponse = {
	access_token: string;
	expires_in: number;
	token_type: string;
};

export type Wallet = {
	walletID: string;
	readonly address: string;
	network: string;
	// the key schema is always ED25519 and qredo is not planning to change it
	publicKey: string;
	labels: {
		key: string;
		name: string;
		value: string;
	}[];
};

export type GetWalletsResponse = {
	wallets: Wallet[];
};

export type GetWalletsParams = {
	filters?: { address?: string };
};

export type NetworkType = 'mainnet' | 'testnet' | 'devnet';

export type TransactionStatus =
	| 'pending'
	| 'created'
	| 'authorized'
	| 'approved'
	| 'expired'
	| 'cancelled'
	| 'rejected'
	| 'signed'
	| 'scheduled'
	| 'pushed'
	| 'confirmed'
	| 'mined'
	| 'failed';

export type PostTransactionParams = {
	messageWithIntent: string;
	broadcast: boolean;
	network: NetworkType;
	from: string;
};

export type TransactionInfoResponse = {
	txID: string;
	txHash: string;
	status: TransactionStatus;
	MessageWithIntent: string;
	sig: string;
	timestamps: Partial<Record<TransactionStatus, number>>;
	events: {
		id: string;
		timestamp: number;
		status: TransactionStatus;
		message: string;
	}[];
	from: string;
	network: string;
	createdBy: string;
	accountID: string;
};

export type GetTransactionsParams = {
	network?: NetworkType;
	/** Filter by address or part of address */
	address?: string;
	/** Qredo wallet id */
	wallet?: string;
};

export type GetTransactionsItem = {
	walletID: string;
	txID: string;
	txHash: string;
	status: TransactionStatus;
};

export type GetTransactionsResponse = {
	list: GetTransactionsItem[];
};

export type AccessTokenRenewalFunction = (qredoID: string) => Promise<string | null>;

const MAX_TRIES_TO_RENEW_ACCESS_TOKEN = 1;

export class QredoAPI {
	readonly baseURL: string;
	readonly qredoID: string;
	#accessToken: string | null;
	#renewAccessTokenFN: AccessTokenRenewalFunction | null;
	#accessTokenRenewInProgress: ReturnType<AccessTokenRenewalFunction> | null = null;

	constructor(
		qredoID: string,
		baseURL: string,
		options: {
			accessToken?: string;
			accessTokenRenewalFN?: AccessTokenRenewalFunction;
		} = {},
	) {
		this.qredoID = qredoID;
		this.baseURL = baseURL + (baseURL.endsWith('/') ? '' : '/');
		this.#accessToken = options.accessToken || null;
		this.#renewAccessTokenFN = options.accessTokenRenewalFN || null;
	}

	public set accessToken(accessToken: string) {
		this.#accessToken = accessToken;
	}

	public get accessToken() {
		return this.#accessToken || '';
	}

	public createAccessToken({
		refreshToken,
		grantType = 'refresh_token',
	}: AccessTokenParams): Promise<AccessTokenResponse> {
		const params = new FormData();
		params.append('refresh_token', refreshToken);
		if (grantType) {
			params.append('grant_type', grantType);
		}
		return this.#request(`${this.baseURL}token`, {
			method: 'post',
			body: params,
		});
	}

	public getWallets({ filters }: GetWalletsParams = {}): Promise<GetWalletsResponse> {
		const searchParams = new URLSearchParams();
		if (filters?.address) {
			searchParams.append('address', filters.address);
		}
		return this.#request(`${this.baseURL}wallets${toSearchQueryString(searchParams)}`);
	}

	public createTransaction(params: PostTransactionParams): Promise<TransactionInfoResponse> {
		return this.#request(`${this.baseURL}transactions`, {
			method: 'post',
			body: JSON.stringify(params),
			headers: {
				'Content-Type': 'application/json',
			},
		});
	}

	public getTransaction(transactionID: string): Promise<TransactionInfoResponse> {
		return this.#request(`${this.baseURL}transactions/${transactionID}`);
	}

	public getTransactions(params: GetTransactionsParams): Promise<GetTransactionsResponse> {
		return this.#request(
			`${this.baseURL}transactions${toSearchQueryString(new URLSearchParams(params))}`,
		);
	}

	async #renewAccessToken() {
		if (!this.#renewAccessTokenFN) {
			return false;
		}
		if (!this.#accessTokenRenewInProgress) {
			this.#accessTokenRenewInProgress = this.#renewAccessTokenFN(this.qredoID).finally(
				() => (this.#accessTokenRenewInProgress = null),
			);
		}
		this.#accessToken = await this.#accessTokenRenewInProgress;
		return !!this.#accessToken;
	}

	#request = async (...params: Parameters<typeof fetch>) => {
		let tries = 0;
		while (tries++ <= MAX_TRIES_TO_RENEW_ACCESS_TOKEN) {
			// TODO: add monitoring?
			const response = await fetch(params[0], {
				...params[1],
				headers: {
					...params[1]?.headers,
					Authorization: `Bearer ${this.#accessToken}`,
				},
			});
			const dataJson = await response.json();
			if (response.ok) {
				return dataJson;
			}
			if (response.status === 401 && tries <= MAX_TRIES_TO_RENEW_ACCESS_TOKEN) {
				if (await this.#renewAccessToken()) {
					// skip the rest and retry the request with the new access token
					continue;
				}
				throw new QredoAPIUnauthorizedError(response.status, dataJson);
			}
			throw new QredoAPIError(response.status, dataJson);
		}
	};
}
