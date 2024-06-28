// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';
import { toB64 } from '@mysten/sui/utils';
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

import type { Encryption } from '../encryption.js';
import type { EnokiClientConfig } from '../EnokiClient/index.js';
import type { EnokiNetwork } from '../EnokiClient/type.js';
import type { AuthProvider } from '../EnokiFlow.js';
import { EnokiFlow } from '../EnokiFlow.js';
import type { SyncStore } from '../stores.js';

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
		name: 'Sign in with Google',
		icon: 'data:image/svg+xml;base64,PHN2ZyBmaWxsPSJub25lIiBoZWlnaHQ9IjMyIiB2aWV3Qm94PSIwIDAgMzIgMzIiIHdpZHRoPSIzMiIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj48cGF0aCBkPSJtMzIgMGgtMzJ2MzJoMzJ6IiBmaWxsPSIjZmZmIi8+PGcgY2xpcC1ydWxlPSJldmVub2RkIiBmaWxsLXJ1bGU9ImV2ZW5vZGQiPjxwYXRoIGQ9Im0yMy44Mjk5IDE2LjE4MThjMC0uNTY3Mi0uMDUwOS0xLjExMjctLjE0NTQtMS42MzYzaC03LjUzNDZ2My4wOTQ1aDQuMzA1NWMtLjE4NTUgMS0uNzQ5MSAxLjg0NzMtMS41OTY0IDIuNDE0NnYyLjAwNzNoMi41ODU1YzEuNTEyNy0xLjM5MjggMi4zODU0LTMuNDQzNyAyLjM4NTQtNS44ODAxeiIgZmlsbD0iIzQyODVmNCIvPjxwYXRoIGQ9Im0xNi4xNDk2IDI0YzIuMTYgMCAzLjk3MDktLjcxNjQgNS4yOTQ2LTEuOTM4MmwtMi41ODU1LTIuMDA3M2MtLjcxNjQuNDgtMS42MzI3Ljc2MzYtMi43MDkxLjc2MzYtMi4wODM2IDAtMy44NDczLTEuNDA3Mi00LjQ3NjQtMy4yOTgxaC0yLjY3MjcxdjIuMDcyN2MxLjMxNjQxIDIuNjE0NSA0LjAyMTgxIDQuNDA3MyA3LjE0OTExIDQuNDA3M3oiIGZpbGw9IiMzNGE4NTMiLz48cGF0aCBkPSJtMTEuNjczNSAxNy41MmMtLjE2LS40OC0uMjUwOS0uOTkyOC0uMjUwOS0xLjUyIDAtLjUyNzMuMDkwOS0xLjA0LjI1MDktMS41MnYtMi4wNzI4aC0yLjY3MjY5Yy0uNTQxODIgMS4wOC0uODUwOTEgMi4zMDE4LS44NTA5MSAzLjU5MjggMCAxLjI5MDkuMzA5MDkgMi41MTI3Ljg1MDkxIDMuNTkyN3oiIGZpbGw9IiNmYmJjMDUiLz48cGF0aCBkPSJtMTYuMTQ5NiAxMS4xODE4YzEuMTc0NSAwIDIuMjI5MS40MDM3IDMuMDU4MiAxLjE5NjRsMi4yOTQ1LTIuMjk0NmMtMS4zODU0LTEuMjkwODctMy4xOTYzLTIuMDgzNi01LjM1MjctMi4wODM2LTMuMTI3MyAwLTUuODMyNyAxLjc5MjczLTcuMTQ5MTEgNC40MDczbDIuNjcyNzEgMi4wNzI3Yy42MjkxLTEuODkwOSAyLjM5MjgtMy4yOTgyIDQuNDc2NC0zLjI5ODJ6IiBmaWxsPSIjZWE0MzM1Ii8+PC9nPjwvc3ZnPg==',
	},
	{
		provider: 'facebook',
		name: 'Sign in with Facebook',
		icon: 'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciCiAgZmlsbD0iIzE4NzdGMiIgdmVyc2lvbj0iMS4wIiB2aWV3Qm94PSIwIDAgMjA4NCAyMDg0Ij48cGF0aCBkPSJNOTkyIDJDODU4LjcgOS4xIDczNi42IDM4LjEgNjE5IDkwLjVjLTI4NS41IDEyNy4yLTQ5OS4xIDM3NS45LTU4MS41IDY3Ny0yNS45IDk0LjYtMzcuOCAxOTAuMi0zNi4yIDI5MSAuOCA0Ni43IDIuOCA3NS4zIDguMyAxMTguNSAxNi4xIDEyNi42IDU2LjcgMjUxLjUgMTE4IDM2My44IDEwMS44IDE4NiAyNTYuOSAzMzYuMiA0NDUuOSA0MzEuNyA2Mi4xIDMxLjMgMTI3LjggNTYuNiAxOTMgNzQuMyA5LjkgMi43IDE5LjIgNS4yIDIwLjggNS42bDIuNy42di02OTJsLTEwNy4yLS4yLTEwNy4zLS4zdi0zMThsMTA3LjEtLjMgMTA3LjItLjIuNS05Mi44Yy41LTkwIC45LTEwMyA0LjMtMTM5LjIgMTctMTgzLjIgOTAtMzA1LjUgMjIwLjUtMzY5LjUgNTguNy0yOC44IDEyOC4zLTQ1LjcgMjE1LjktNTIuNSAyMi44LTEuOCA4Mi40LTIuNCAxMDYtMS4xIDU3LjEgMy4yIDEyMC40IDEwLjYgMTYzIDE5LjEgMTAuNyAyLjIgMjAuOSA0LjMgMjIuNSA0LjhsMyAuOC4zIDE0NC45LjIgMTQ0LjgtNi4yLS42Yy0yOS4zLTMtMTMzLjEtNC4yLTE1OC4zLTEuOS02NS42IDYtMTA4LjYgMjIuMy0xMzkgNTIuNy0yMi45IDIyLjktMzcuOCA1My00NS45IDkyLjgtNi40IDMxLjEtNy42IDUyLjgtNy42IDEzMi45djY0LjhoMTcwYzkzLjUgMCAxNzAgLjQgMTcwIC44IDAgLjUtMTMgNzEuOS0yOSAxNTguNy0xNS45IDg2LjgtMjkgMTU4LjItMjkgMTU4LjcgMCAuNC02My40LjgtMTQxIC44aC0xNDF2MzU3LjVjMCAyODUuMy4zIDM1Ny41IDEuMyAzNTcuNSAzLjMgMCA0NC43LTYuNCA2MS42LTkuNSAxNjMtMjkuOSAzMTYuNy05OC44IDQ0OS4xLTIwMS40IDU1LjgtNDMuMiAxMTMuOS05OS4xIDE1OS42LTE1My43IDQxLjMtNDkuMSA4MC41LTEwNi4yIDExMi44LTE2My45IDE5LjctMzUuMiA0Ny05My42IDYxLjctMTMyLjMgNzAuNi0xODQuOCA4Ny4yLTM4Ni4xIDQ3LjgtNTgxLjUtNDUuNy0yMjYuNi0xNjkuNC00MzUuNi0zNDYuOS01ODUuOC0xNDQuNS0xMjIuMi0zMTYuNC0yMDItNTAxLjUtMjMyLjktMzEuMy01LjItNjYuNC05LjItMTA0LTEyLTE4LjMtMS40LTk4LjctMi4xLTExOC41LTF6Ii8+PC9zdmc+',
	},
	{
		provider: 'twitch',
		name: 'Sign in with Twitch',
		icon: 'data:image/svg+xml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBlbmNvZGluZz0idXRmLTgiPz4KPCEtLSBHZW5lcmF0b3I6IEFkb2JlIElsbHVzdHJhdG9yIDIzLjAuNiwgU1ZHIEV4cG9ydCBQbHVnLUluIC4gU1ZHIFZlcnNpb246IDYuMDAgQnVpbGQgMCkgIC0tPgo8c3ZnIHZlcnNpb249IjEuMSIgaWQ9IkxheWVyXzEiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgeG1sbnM6eGxpbms9Imh0dHA6Ly93d3cudzMub3JnLzE5OTkveGxpbmsiCgl2aWV3Qm94PSItNDAwIC00MDAgMjgwMCAzMjAwIiB4bWw6c3BhY2U9InByZXNlcnZlIj4KCTxzdHlsZSB0eXBlPSJ0ZXh0L2NzcyI+CgkJLnN0MCB7CgkJCWZpbGw6ICNGRkZGRkY7CgkJfQoKCQkuc3QxIHsKCQkJZmlsbDogIzkxNDZGRjsKCQl9Cgk8L3N0eWxlPgoJPHRpdGxlPkFzc2V0IDI8L3RpdGxlPgoJPGc+CgkJPHBvbHlnb24gY2xhc3M9InN0MCIgcG9pbnRzPSIyMjAwLDEzMDAgMTgwMCwxNzAwIDE0MDAsMTcwMCAxMDUwLDIwNTAgMTA1MCwxNzAwIDYwMCwxNzAwIDYwMCwyMDAgMjIwMCwyMDAgCSIgLz4KCQk8Zz4KCQkJPGcgaWQ9IkxheWVyXzEtMiI+CgkJCQk8cGF0aCBjbGFzcz0ic3QxIiBkPSJNNTAwLDBMMCw1MDB2MTgwMGg2MDB2NTAwbDUwMC01MDBoNDAwbDkwMC05MDBWMEg1MDB6IE0yMjAwLDEzMDBsLTQwMCw0MDBoLTQwMGwtMzUwLDM1MHYtMzUwSDYwMFYyMDBoMTYwMAoJCQkJVjEzMDB6IiAvPgoJCQkJPHJlY3QgeD0iMTcwMCIgeT0iNTUwIiBjbGFzcz0ic3QxIiB3aWR0aD0iMjAwIiBoZWlnaHQ9IjYwMCIgLz4KCQkJCTxyZWN0IHg9IjExNTAiIHk9IjU1MCIgY2xhc3M9InN0MSIgd2lkdGg9IjIwMCIgaGVpZ2h0PSI2MDAiIC8+CgkJCTwvZz4KCQk8L2c+Cgk8L2c+Cjwvc3ZnPgo=',
	},
];

export class EnokiWallet implements Wallet {
	#events: Emitter<WalletEventsMap>;
	#accounts: ReadonlyWalletAccount[];
	#name: string;
	#id: string;
	#icon: Wallet['icon'];
	#flow: EnokiFlow;
	#provider: AuthProvider;
	#clientId: string;
	#redirectUrl: string | undefined;
	#extraParams: Record<string, string> | undefined;
	#network: EnokiNetwork;
	#client;

	get id() {
		return this.#id;
	}

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
		extraParams,
		client,
		network,
	}: {
		icon: Wallet['icon'];
		name: string;
		flow: EnokiFlow;
		provider: AuthProvider;
		clientId: string;
		redirectUrl?: string;
		extraParams?: Record<string, string>;
		client: SuiClient;
		network: EnokiNetwork;
	}) {
		this.#accounts = [];
		this.#events = mitt();

		this.#client = client;
		this.#name = name;
		this.#id = `enoki:${provider}:${network}:${clientId}`;
		this.#icon = icon;
		this.#flow = flow;
		this.#provider = provider;
		this.#clientId = clientId;
		this.#redirectUrl = redirectUrl;
		this.#extraParams = extraParams;
		this.#network = network;

		this.#setAccount();
	}

	#signTransaction: SuiSignTransactionMethod = async ({ transaction }) => {
		const parsedTransaction = Transaction.from(await transaction.toJSON());
		const keypair = await this.#flow.getKeypair({ network: this.#network });

		return keypair.signTransaction(await parsedTransaction.build({ client: this.#client }));
	};

	#signAndExecuteTransaction: SuiSignAndExecuteTransactionMethod = async ({ transaction }) => {
		const parsedTransaction = Transaction.from(await transaction.toJSON());

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
			extraParams: this.#extraParams,
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

export interface RegisterEnokiWalletsOptions extends EnokiClientConfig {
	/**
	 * The storage interface to persist Enoki data locally.
	 * If not provided, it will use a sessionStorage-backed store.
	 */
	store?: SyncStore;
	/**
	 * The encryption interface that will be used to encrypt data before storing it locally.
	 * If not provided, it will use a default encryption interface.
	 */
	encryption?: Encryption;
	/**
	 * Conviguration for each OAuth provider.
	 */
	providers: Partial<
		Record<
			AuthProvider,
			{
				/**
				 * The OAuth client ID.
				 */
				clientId: string;
				/**
				 * The URL to redirect to after authorization.
				 */
				redirectUrl?: string;
				/**
				 * Extra parameters to include in the authorization URL.
				 */
				extraParams?: Record<string, string>;
			}
		>
	>;
	/**
	 * The SuiClient instance to use when building and executing transactions.
	 */
	client: SuiClient;
	/**
	 * The network to use when building and executing transactions (default: 'mainnet')
	 */
	network?: string;
}

export function registerEnokiWallets({
	providers,
	client,
	network = 'mainnet',
	...config
}: RegisterEnokiWalletsOptions) {
	const walletsApi = getWallets();
	const flow = new EnokiFlow(config);

	const unregisterCallbacks: (() => void)[] = [];
	const wallets: Partial<Record<AuthProvider, EnokiWallet>> = {};

	if (network === 'mainnet' || network === 'testnet' || network === 'devnet') {
		for (const { name, icon, provider } of ENOKI_PROVIDER_WALLETS_INFO) {
			const providerOptions = providers[provider];
			if (providerOptions) {
				const { clientId, redirectUrl, extraParams } = providerOptions;
				const wallet = new EnokiWallet({
					name,
					icon,
					flow,
					provider,
					clientId,
					client,
					redirectUrl,
					extraParams,
					network,
				});
				const unregister = walletsApi.register(wallet);

				unregisterCallbacks.push(unregister);
				wallets[provider] = wallet;
			}
		}
	}

	return {
		wallets,
		unregister: () => {
			for (const unregister of unregisterCallbacks) {
				unregister();
			}
		},
	};
}

export function isEnokiWallet(wallet: Wallet): wallet is EnokiWallet {
	return !!wallet.id?.startsWith('enoki:');
}
