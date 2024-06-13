// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';
import type {
	IdentifierArray,
	StandardConnectFeature,
	StandardConnectMethod,
	StandardDisconnectFeature,
	StandardDisconnectMethod,
	StandardEventsFeature,
	StandardEventsListeners,
	StandardEventsOnMethod,
	SuiSignAndExecuteTransactionFeature,
	SuiSignAndExecuteTransactionMethod,
	SuiSignPersonalMessageFeature,
	SuiSignPersonalMessageMethod,
	SuiSignTransactionFeature,
	SuiSignTransactionMethod,
	Wallet,
} from '@mysten/wallet-standard';
import { getWallets, ReadonlyWalletAccount } from '@mysten/wallet-standard';
import type { Emitter } from 'mitt';
import mitt from 'mitt';

import { fromB64, toB64 } from '../../../bcs/src/b64.js';
import type { EnokiNetwork } from '../EnokiClient/type.js';
import type { AuthProvider, EnokiFlowConfig } from '../EnokiFlow.js';
import { EnokiFlow } from '../EnokiFlow.js';

type WalletEventsMap = {
	[E in keyof StandardEventsListeners]: Parameters<StandardEventsListeners[E]>[0];
};

const ENOKI_PROVIDER_WALLETS_INFO: {
	name: string;
	icon: Wallet['icon'];
	provider: AuthProvider;
}[] = [
	{
		provider: 'google',
		name: 'Google',
		icon: 'data:image/svg+xml;base64,PHN2ZyBmaWxsPSJub25lIiBoZWlnaHQ9IjMyIiB2aWV3Qm94PSIwIDAgMzIgMzIiIHdpZHRoPSIzMiIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48cGF0aCBkPSJtMzIgMGgtMzJ2MzJoMzJ6IiBmaWxsPSIjZmZmIi8+PGcgY2xpcC1ydWxlPSJldmVub2RkIiBmaWxsLXJ1bGU9ImV2ZW5vZGQiPjxwYXRoIGQ9Im0yMy44Mjk5IDE2LjE4MThjMC0uNTY3Mi0uMDUwOS0xLjExMjctLjE0NTQtMS42MzYzaC03LjUzNDZ2My4wOTQ1aDQuMzA1NWMtLjE4NTUgMS0uNzQ5MSAxLjg0NzMtMS41OTY0IDIuNDE0NnYyLjAwNzNoMi41ODU1YzEuNTEyNy0xLjM5MjggMi4zODU0LTMuNDQzNyAyLjM4NTQtNS44ODAxeiIgZmlsbD0iIzQyODVmNCIvPjxwYXRoIGQ9Im0xNi4xNDk2IDI0YzIuMTYgMCAzLjk3MDktLjcxNjQgNS4yOTQ2LTEuOTM4MmwtMi41ODU1LTIuMDA3M2MtLjcxNjQuNDgtMS42MzI3Ljc2MzYtMi43MDkxLjc2MzYtMi4wODM2IDAtMy44NDczLTEuNDA3Mi00LjQ3NjQtMy4yOTgxaC0yLjY3MjcxdjIuMDcyN2MxLjMxNjQxIDIuNjE0NSA0LjAyMTgxIDQuNDA3MyA3LjE0OTExIDQuNDA3M3oiIGZpbGw9IiMzNGE4NTMiLz48cGF0aCBkPSJtMTEuNjczNSAxNy41MmMtLjE2LS40OC0uMjUwOS0uOTkyOC0uMjUwOS0xLjUyIDAtLjUyNzMuMDkwOS0xLjA0LjI1MDktMS41MnYtMi4wNzI4aC0yLjY3MjY5Yy0uNTQxODIgMS4wOC0uODUwOTEgMi4zMDE4LS44NTA5MSAzLjU5MjggMCAxLjI5MDkuMzA5MDkgMi41MTI3Ljg1MDkxIDMuNTkyN3oiIGZpbGw9IiNmYmJjMDUiLz48cGF0aCBkPSJtMTYuMTQ5NiAxMS4xODE4YzEuMTc0NSAwIDIuMjI5MS40MDM3IDMuMDU4MiAxLjE5NjRsMi4yOTQ1LTIuMjk0NmMtMS4zODU0LTEuMjkwODctMy4xOTYzLTIuMDgzNi01LjM1MjctMi4wODM2LTMuMTI3MyAwLTUuODMyNyAxLjc5MjczLTcuMTQ5MTEgNC40MDczbDIuNjcyNzEgMi4wNzI3Yy42MjkxLTEuODkwOSAyLjM5MjgtMy4yOTgyIDQuNDc2NC0zLjI5ODJ6IiBmaWxsPSIjZWE0MzM1Ii8+PC9nPjwvc3ZnPg==',
	},
];

export class EnokiWallet implements Wallet {
	#events: Emitter<WalletEventsMap>;
	#accounts: ReadonlyWalletAccount[];
	#name: string;
	#icon: Wallet['icon'];
	#flow: EnokiFlow;
	#provider: AuthProvider;
	#clientId: string;
	#redirectUrl: string | undefined;
	#network: EnokiNetwork;
	#sponsor;
	#client;

	get name() {
		return this.#name;
	}

	get icon() {
		return this.#icon;
	}

	get version() {
		return '1.0.0' as const;
	}

	get chains() {
		return [`sui:${this.#network}`] as const;
	}

	get accounts() {
		return this.#accounts;
	}

	get features(): StandardConnectFeature &
		StandardDisconnectFeature &
		StandardEventsFeature &
		SuiSignTransactionFeature &
		SuiSignAndExecuteTransactionFeature &
		SuiSignPersonalMessageFeature {
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
			'sui:signTransaction': {
				version: '2.0.0',
				signTransaction: this.#signTransaction,
			},
			'sui:signAndExecuteTransaction': {
				version: '2.0.0',
				signAndExecuteTransaction: this.#signAndExecuteTransaction,
			},
			'sui:signPersonalMessage': {
				version: '1.0.0',
				signPersonalMessage: this.#signPersonalMessage,
			},
		};
	}

	constructor({
		name,
		icon,
		flow,
		provider,
		clientId,
		redirectUrl,
		client,
		network,
		sponsor,
	}: {
		icon: Wallet['icon'];
		name: string;
		flow: EnokiFlow;
		provider: AuthProvider;
		clientId: string;
		redirectUrl?: string;
		client: SuiClient;
		network: EnokiNetwork;
		sponsor: boolean;
	}) {
		this.#accounts = [];
		this.#events = mitt();

		this.#client = client;
		this.#name = name;
		this.#icon = icon;
		this.#flow = flow;
		this.#provider = provider;
		this.#clientId = clientId;
		this.#redirectUrl = redirectUrl;
		this.#network = network;
		this.#sponsor = sponsor;

		this.#setAccount();
	}

	#signTransaction: SuiSignTransactionMethod = async ({ transaction }) => {
		const parsedTransaction = Transaction.from(await transaction.toJSON());
		const keypair = await this.#flow.getKeypair({ network: this.#network });

		if (this.#sponsor) {
			const { bytes } = await this.#flow.sponsorTransaction({
				network: this.#network,
				client: this.#client,
				transaction: parsedTransaction,
			});
			return keypair.signTransaction(fromB64(bytes));
		}

		return keypair.signTransaction(await parsedTransaction.build({ client: this.#client }));
	};

	#signAndExecuteTransaction: SuiSignAndExecuteTransactionMethod = async ({ transaction }) => {
		const parsedTransaction = Transaction.from(await transaction.toJSON());

		if (this.#sponsor) {
			return this.#flow.sponsorAndExecuteTransaction({
				network: this.#network,
				client: this.#client,
				transaction: parsedTransaction,
			});
		}

		const keypair = await this.#flow.getKeypair({ network: this.#network });
		parsedTransaction.setSenderIfNotSet(keypair.toSuiAddress());
		const bytes = await parsedTransaction.build({ client: this.#client });
		const { signature } = await keypair.signTransaction(
			await parsedTransaction.build({ client: this.#client }),
		);

		const { digest, rawEffects } = await this.#client.executeTransactionBlock({
			transactionBlock: bytes,
			signature,
			options: {
				showRawEffects: true,
			},
		});

		return {
			digest,
			signature,
			bytes: toB64(bytes),
			effects: toB64(Uint8Array.from(rawEffects!)),
		};
	};

	#signPersonalMessage: SuiSignPersonalMessageMethod = async ({ message }) => {
		return (await this.#flow.getKeypair()).signPersonalMessage(message);
	};

	#on: StandardEventsOnMethod = (event, listener) => {
		this.#events.on(event, listener);
		return () => this.#events.off(event, listener);
	};

	#setAccount() {
		const state = this.#flow.$zkLoginState.get();
		if (state.address) {
			this.#accounts = [
				new ReadonlyWalletAccount({
					address: state.address,
					chains: this.chains,
					features: Object.keys(this.features) as IdentifierArray,
					// NOTE: Stashed doesn't support getting public keys, and zkLogin accounts don't have meaningful public keys anyway
					publicKey: new Uint8Array(),
				}),
			];
		} else {
			this.#accounts = [];
		}

		this.#events.emit('change', { accounts: this.accounts });
	}

	#connect: StandardConnectMethod = async (input) => {
		this.#setAccount();

		if (this.accounts.length || input?.silent) {
			return { accounts: this.accounts };
		}

		const popup = window.open();
		if (!popup) {
			throw new Error('Failed to open popup');
		}

		const url = await this.#flow.createAuthorizationURL({
			provider: this.#provider,
			clientId: this.#clientId,
			redirectUrl: this.#redirectUrl ?? window.location.href.split('#')[0],
			network: this.#network,
		});

		popup.location = url;

		await new Promise<void>((resolve, reject) => {
			const interval = setInterval(() => {
				try {
					if (popup.closed) {
						clearInterval(interval);
						reject(new Error('Popup closed'));
					}

					if (!popup.location.hash) {
						return;
					}
				} catch (e) {
					return;
				}
				clearInterval(interval);

				this.#flow.handleAuthCallback(popup.location.hash).then(() => resolve(), reject);

				try {
					popup.close();
				} catch (e) {
					console.error(e);
				}
			}, 16);
		});

		this.#setAccount();

		return { accounts: this.accounts };
	};

	#disconnect: StandardDisconnectMethod = async () => {
		await this.#flow.logout();

		this.#setAccount();
	};
}

export function registerEnokiWallets({
	clientIds,
	redirectUrl,
	client,
	network = 'mainnet',
	sponsor = true,
	...config
}: EnokiFlowConfig & {
	clientIds: Partial<Record<AuthProvider, string>>;
	redirectUrl?: string;
	client: SuiClient;
	network?: string;
	sponsor?: boolean;
}) {
	const walletsApi = getWallets();
	const flow = new EnokiFlow(config);

	const unregisterCallbacks: (() => void)[] = [];
	const wallets: Partial<Record<AuthProvider, EnokiWallet>> = {};

	if (network === 'mainnet' || network === 'testnet' || network === 'devnet') {
		for (const { name, icon, provider } of ENOKI_PROVIDER_WALLETS_INFO) {
			const clientId = clientIds[provider];
			if (clientId) {
				const wallet = new EnokiWallet({
					name,
					icon,
					flow,
					provider,
					clientId,
					client,
					redirectUrl,
					network,
					sponsor,
				});
				const unregister = walletsApi.register(wallet);

				unregisterCallbacks.push(unregister);
				wallets[provider] = wallet;
			}
		}
	}

	return {
		wallets,
		flow,
		unregister: () => {
			for (const unregister of unregisterCallbacks) {
				unregister();
			}
		},
	};
}
