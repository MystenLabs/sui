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
import type { Mock } from 'vitest';

export class MockWallet implements Wallet {
	version = '1.0.0' as const;
	icon = `data:image/png;base64,` as const;
	chains = SUI_CHAINS;

	mocks: {
		connect: Mock;
		disconnect: Mock;
	};

	#walletName: string;
	#accounts: ReadonlyWalletAccount[];
	#features: IdentifierRecord<unknown>;
	#eventHandlers: {
		event: string;
		listener: (properties: StandardEventsChangeProperties) => void;
	}[];

	#on = vi.fn((...args: Parameters<StandardEventsOnMethod>) => {
		this.#eventHandlers.push({ event: args[0], listener: args[1] });
		return () => {
			this.#eventHandlers = [];
		};
	});

	readonly id?: string;

	constructor(
		id: string | null | undefined,
		name: string,
		accounts: ReadonlyWalletAccount[],
		features: IdentifierRecord<unknown>,
	) {
		if (id) {
			this.id = id;
		}

		this.#walletName = name;
		this.#accounts = accounts;
		this.#features = features;
		this.#eventHandlers = [];
		this.mocks = {
			connect: vi.fn().mockImplementation(() => ({ accounts: this.#accounts })),
			disconnect: vi.fn().mockImplementation(() => {}),
		};
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
				connect: this.mocks.connect,
			},
			'standard:disconnect': {
				version: '1.0.0',
				disconnect: this.mocks.disconnect,
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
