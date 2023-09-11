// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type {
	IdentifierRecord,
	StandardConnectFeature,
	StandardEventsFeature,
	SuiFeatures,
	ReadonlyWalletAccount,
} from '@mysten/wallet-standard';
import { SUI_CHAINS } from '@mysten/wallet-standard';
import type { Wallet } from '@mysten/wallet-standard';

export class MockWallet implements Wallet {
	version = '1.0.0' as const;
	icon = `data:image/png;base64,` as const;
	chains = SUI_CHAINS;
	#walletName: string;
	#accounts: ReadonlyWalletAccount[];
	#additionalFeatures: IdentifierRecord<unknown>;

	#connect = vi.fn().mockImplementation(() => ({ accounts: this.#accounts }));
	#disconnect = vi.fn();
	#on = vi.fn();
	#signPersonalMessage = vi.fn();
	#signTransactionBlock = vi.fn();
	#signAndExecuteTransactionBlock = vi.fn();

	constructor(
		name: string,
		accounts: ReadonlyWalletAccount[],
		additionalFeatures: IdentifierRecord<unknown>,
	) {
		this.#walletName = name;
		this.#accounts = accounts;
		this.#additionalFeatures = additionalFeatures;
	}

	get name() {
		return this.#walletName;
	}

	get accounts() {
		return this.#accounts;
	}

	get features(): StandardConnectFeature &
		StandardEventsFeature &
		SuiFeatures &
		IdentifierRecord<unknown> {
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
			'sui:signPersonalMessage': {
				version: '1.0.0',
				signPersonalMessage: this.#signPersonalMessage,
			},
			'sui:signTransactionBlock': {
				version: '1.0.0',
				signTransactionBlock: this.#signTransactionBlock,
			},
			'sui:signAndExecuteTransactionBlock': {
				version: '1.0.0',
				signAndExecuteTransactionBlock: this.#signAndExecuteTransactionBlock,
			},
			...this.#additionalFeatures,
		};
	}
}
