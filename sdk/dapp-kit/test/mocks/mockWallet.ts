// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	IdentifierRecord,
	ReadonlyWalletAccount,
	StandardEventsChangeProperties,
	StandardEventsOnMethod,
	Wallet,
	WalletWithRequiredFeatures,
} from '@mysten/wallet-standard';
import { SUI_CHAINS } from '@mysten/wallet-standard';

export class MockWallet implements Wallet {
	version = '1.0.0' as const;
	icon = `data:image/png;base64,` as const;
	chains = SUI_CHAINS;

	#walletName: string;
	#accounts: ReadonlyWalletAccount[];
	#features: IdentifierRecord<unknown>;
	#eventHandlers: {
		event: string;
		listener: (properties: StandardEventsChangeProperties) => void;
	}[];

	#connect = vi.fn().mockImplementation(() => ({ accounts: this.#accounts }));
	#disconnect = vi.fn();

	#on = vi.fn((...args: Parameters<StandardEventsOnMethod>) => {
		this.#eventHandlers.push({ event: args[0], listener: args[1] });
		return () => {
			this.#eventHandlers = [];
		};
	});

	constructor(
		name: string,
		accounts: ReadonlyWalletAccount[],
		features: IdentifierRecord<unknown>,
	) {
		this.#walletName = name;
		this.#accounts = accounts;
		this.#features = features;
		this.#eventHandlers = [];
	}

	get name() {
		return this.#walletName;
	}

	get accounts() {
		return this.#accounts;
	}

	get features(): WalletWithRequiredFeatures['features'] {
		return {
			'standard:connect': {
				version: '1.0.0',
				connect: this.#connect,
			},
			'standard:disconnect': {
				version: '1.0.0',
				disconnect: this.#disconnect,
			},
			'standard:events': {
				version: '1.0.0',
				on: this.#on,
			},
			...this.#features,
		};
	}

	deleteFirstAccount() {
		this.#accounts.splice(0, 1);
		this.#eventHandlers.forEach(({ listener }) => {
			listener({ accounts: this.#accounts });
		});
	}
}
