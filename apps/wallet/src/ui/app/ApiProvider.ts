// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type AccountType, type SerializedUIAccount } from '_src/background/accounts/Account';
import { API_ENV } from '_src/shared/api-env';
import { getSuiClient } from '_src/shared/sui-client';
import { type SuiClient } from '@mysten/sui/client';

import type { BackgroundClient } from './background-client';
import { BackgroundServiceSigner } from './background-client/BackgroundServiceSigner';
import { queryClient } from './helpers/queryClient';
import { type WalletSigner } from './WalletSigner';

type EnvInfo = {
	name: string;
	env: API_ENV;
};

export const API_ENV_TO_INFO: Record<API_ENV, EnvInfo> = {
	[API_ENV.local]: { name: 'Local', env: API_ENV.local },
	[API_ENV.devNet]: { name: 'Devnet', env: API_ENV.devNet },
	[API_ENV.customRPC]: { name: 'Custom RPC', env: API_ENV.customRPC },
	[API_ENV.testNet]: { name: 'Testnet', env: API_ENV.testNet },
	[API_ENV.mainnet]: { name: 'Mainnet', env: API_ENV.mainnet },
};

function getDefaultApiEnv() {
	const apiEnv = process.env.API_ENV;
	if (apiEnv && !Object.keys(API_ENV).includes(apiEnv)) {
		throw new Error(`Unknown environment variable API_ENV, ${apiEnv}`);
	}
	return apiEnv ? API_ENV[apiEnv as keyof typeof API_ENV] : API_ENV.devNet;
}

export const DEFAULT_API_ENV = getDefaultApiEnv();

type NetworkTypes = keyof typeof API_ENV;

export const generateActiveNetworkList = (): NetworkTypes[] => {
	return Object.values(API_ENV);
};

const accountTypesWithBackgroundSigner: AccountType[] = ['mnemonic-derived', 'imported', 'zkLogin'];

export default class ApiProvider {
	private _apiFullNodeProvider?: SuiClient;
	private _signerByAddress: Map<string, WalletSigner> = new Map();
	apiEnv: API_ENV = DEFAULT_API_ENV;

	public setNewJsonRpcProvider(apiEnv: API_ENV = DEFAULT_API_ENV, customRPC?: string | null) {
		this.apiEnv = apiEnv;
		this._apiFullNodeProvider = getSuiClient(
			apiEnv === API_ENV.customRPC
				? { env: apiEnv, customRpcUrl: customRPC || '' }
				: { env: apiEnv, customRpcUrl: null },
		);

		this._signerByAddress.clear();

		// We also clear the query client whenever set set a new API provider:
		queryClient.resetQueries();
		queryClient.clear();
	}

	public get instance() {
		if (!this._apiFullNodeProvider) {
			this.setNewJsonRpcProvider();
		}
		return {
			// eslint-disable-next-line @typescript-eslint/no-non-null-assertion
			fullNode: this._apiFullNodeProvider!,
		};
	}

	public getSignerInstance(
		account: SerializedUIAccount,
		backgroundClient: BackgroundClient,
	): WalletSigner {
		if (!this._apiFullNodeProvider) {
			this.setNewJsonRpcProvider();
		}
		if (accountTypesWithBackgroundSigner.includes(account.type)) {
			return this.getBackgroundSignerInstance(account, backgroundClient);
		}
		if ('ledger' === account.type) {
			// Ideally, Ledger transactions would be signed in the background
			// and exist as an asynchronous keypair; however, this isn't possible
			// because you can't connect to a Ledger device from the background
			// script. Similarly, the signer instance can't be retrieved from
			// here because ApiProvider is a global and results in very buggy
			// behavior due to the reactive nature of managing Ledger connections
			// and displaying relevant UI updates. Refactoring ApiProvider to
			// not be a global instance would help out here, but that is also
			// a non-trivial task because we need access to ApiProvider in the
			// background script as well.
			throw new Error("Signing with Ledger via ApiProvider isn't supported");
		}
		throw new Error('Encountered unknown account type');
	}

	public getBackgroundSignerInstance(
		account: SerializedUIAccount,
		backgroundClient: BackgroundClient,
	): WalletSigner {
		const key = account.id;
		if (!this._signerByAddress.has(account.id)) {
			this._signerByAddress.set(
				key,
				new BackgroundServiceSigner(account, backgroundClient, this._apiFullNodeProvider!),
			);
		}
		return this._signerByAddress.get(key)!;
	}
}

export const walletApiProvider = new ApiProvider();
