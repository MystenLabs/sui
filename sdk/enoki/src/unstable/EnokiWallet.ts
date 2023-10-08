// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getWallets, SUI_CHAINS } from '@mysten/wallet-standard';
import type {
	StandardConnectFeature,
	StandardConnectMethod,
	StandardEventsFeature,
	StandardEventsOnMethod,
	SuiFeatures,
	SuiSignAndExecuteTransactionBlockMethod,
	SuiSignPersonalMessageMethod,
	SuiSignTransactionBlockMethod,
	Wallet,
} from '@mysten/wallet-standard';

import type { AuthProvider, EnokiFlow } from '../EnokiFlow.js';

const AUTH_PROVIDER_CONFIG = {
	google: {
		name: 'Google',
		icon: 'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIGhlaWdodD0iMjQiIHZpZXdCb3g9IjAgMCAyNCAyNCIgd2lkdGg9IjI0Ij48cGF0aCBkPSJNMjIuNTYgMTIuMjVjMC0uNzgtLjA3LTEuNTMtLjItMi4yNUgxMnY0LjI2aDUuOTJjLS4yNiAxLjM3LTEuMDQgMi41My0yLjIxIDMuMzF2Mi43N2gzLjU3YzIuMDgtMS45MiAzLjI4LTQuNzQgMy4yOC04LjA5eiIgZmlsbD0iIzQyODVGNCIvPjxwYXRoIGQ9Ik0xMiAyM2MyLjk3IDAgNS40Ni0uOTggNy4yOC0yLjY2bC0zLjU3LTIuNzdjLS45OC42Ni0yLjIzIDEuMDYtMy43MSAxLjA2LTIuODYgMC01LjI5LTEuOTMtNi4xNi00LjUzSDIuMTh2Mi44NEMzLjk5IDIwLjUzIDcuNyAyMyAxMiAyM3oiIGZpbGw9IiMzNEE4NTMiLz48cGF0aCBkPSJNNS44NCAxNC4wOWMtLjIyLS42Ni0uMzUtMS4zNi0uMzUtMi4wOXMuMTMtMS40My4zNS0yLjA5VjcuMDdIMi4xOEMxLjQzIDguNTUgMSAxMC4yMiAxIDEycy40MyAzLjQ1IDEuMTggNC45M2wyLjg1LTIuMjIuODEtLjYyeiIgZmlsbD0iI0ZCQkMwNSIvPjxwYXRoIGQ9Ik0xMiA1LjM4YzEuNjIgMCAzLjA2LjU2IDQuMjEgMS42NGwzLjE1LTMuMTVDMTcuNDUgMi4wOSAxNC45NyAxIDEyIDEgNy43IDEgMy45OSAzLjQ3IDIuMTggNy4wN2wzLjY2IDIuODRjLjg3LTIuNiAzLjMtNC41MyA2LjE2LTQuNTN6IiBmaWxsPSIjRUE0MzM1Ii8+PHBhdGggZD0iTTEgMWgyMnYyMkgxeiIgZmlsbD0ibm9uZSIvPjwvc3ZnPg==',
	},
	facebook: {
		name: 'Facebook',
		icon: 'data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIGZpbGw9Im5vbmUiIHZpZXdCb3g9IjAgMCA1MDAgNTAwIiB3aWR0aD0iNTAwIiBoZWlnaHQ9IjUwMCI+CiAgPHBhdGggZD0ibTUwMCwyNTBDNTAwLDExMS45MywzODguMDcsMCwyNTAsMFMwLDExMS45MywwLDI1MGMwLDExNy4yNCw4MC43MiwyMTUuNjIsMTg5LjYxLDI0Mi42NHYtMTY2LjI0aC01MS41NXYtNzYuNGg1MS41NXYtMzIuOTJjMC04NS4wOSwzOC41MS0xMjQuNTMsMTIyLjA1LTEyNC41MywxNS44NCwwLDQzLjE3LDMuMTEsNTQuMzUsNi4yMXY2OS4yNWMtNS45LS42Mi0xNi4xNS0uOTMtMjguODgtLjkzLTQwLjk5LDAtNTYuODMsMTUuNTMtNTYuODMsNTUuOXYyNy4wMmg4MS42NmwtMTQuMDMsNzYuNGgtNjcuNjN2MTcxLjc3YzEyMy43Ny0xNC45NSwyMTkuNy0xMjAuMzUsMjE5LjctMjQ4LjE3WiIgZmlsbD0iIzA4NjZmZiIvPgo8L3N2Zz4=',
	},
	twitch: {
		name: 'Twitch',
		icon: 'data:image/svg+xml;base64,PD94bWwgdmVyc2lvbj0iMS4wIiBlbmNvZGluZz0idXRmLTgiPz4KPCEtLSBHZW5lcmF0b3I6IEFkb2JlIElsbHVzdHJhdG9yIDIzLjAuNiwgU1ZHIEV4cG9ydCBQbHVnLUluIC4gU1ZHIFZlcnNpb246IDYuMDAgQnVpbGQgMCkgIC0tPgo8c3ZnIHZlcnNpb249IjEuMSIgaWQ9IkxheWVyXzEiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgeG1sbnM6eGxpbms9Imh0dHA6Ly93d3cudzMub3JnLzE5OTkveGxpbmsiIHg9IjBweCIgeT0iMHB4IgoJIHZpZXdCb3g9IjAgMCAyNDAwIDI4MDAiIHN0eWxlPSJlbmFibGUtYmFja2dyb3VuZDpuZXcgMCAwIDI0MDAgMjgwMDsiIHhtbDpzcGFjZT0icHJlc2VydmUiPgo8c3R5bGUgdHlwZT0idGV4dC9jc3MiPgoJLnN0MHtmaWxsOiNGRkZGRkY7fQoJLnN0MXtmaWxsOiM5MTQ2RkY7fQo8L3N0eWxlPgo8dGl0bGU+QXNzZXQgMjwvdGl0bGU+CjxnPgoJPHBvbHlnb24gY2xhc3M9InN0MCIgcG9pbnRzPSIyMjAwLDEzMDAgMTgwMCwxNzAwIDE0MDAsMTcwMCAxMDUwLDIwNTAgMTA1MCwxNzAwIDYwMCwxNzAwIDYwMCwyMDAgMjIwMCwyMDAgCSIvPgoJPGc+CgkJPGcgaWQ9IkxheWVyXzEtMiI+CgkJCTxwYXRoIGNsYXNzPSJzdDEiIGQ9Ik01MDAsMEwwLDUwMHYxODAwaDYwMHY1MDBsNTAwLTUwMGg0MDBsOTAwLTkwMFYwSDUwMHogTTIyMDAsMTMwMGwtNDAwLDQwMGgtNDAwbC0zNTAsMzUwdi0zNTBINjAwVjIwMGgxNjAwCgkJCQlWMTMwMHoiLz4KCQkJPHJlY3QgeD0iMTcwMCIgeT0iNTUwIiBjbGFzcz0ic3QxIiB3aWR0aD0iMjAwIiBoZWlnaHQ9IjYwMCIvPgoJCQk8cmVjdCB4PSIxMTUwIiB5PSI1NTAiIGNsYXNzPSJzdDEiIHdpZHRoPSIyMDAiIGhlaWdodD0iNjAwIi8+CgkJPC9nPgoJPC9nPgo8L2c+Cjwvc3ZnPgo=',
	},
} as const;

/**
 * @experimental This API is not yet stable and may change or be removed in any future version.
 */
export function unstable_registerWalletStandard(
	flow: EnokiFlow,
	provider: AuthProvider,
	clientId: string,
	redirectUrl: string,
	extraParams?: Record<string, unknown>,
) {
	const wallets = getWallets();

	class EnokiWallet implements Wallet {
		get version() {
			return '1.0.0' as const;
		}

		get name() {
			return AUTH_PROVIDER_CONFIG[provider].name;
		}

		get icon() {
			return AUTH_PROVIDER_CONFIG[provider].icon;
		}

		get chains() {
			return SUI_CHAINS;
		}

		get accounts() {
			return [];
		}

		get features(): StandardConnectFeature & StandardEventsFeature & SuiFeatures {
			return {
				'standard:connect': {
					version: '1.0.0',
					connect: this.#connect,
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
			};
		}

		#on: StandardEventsOnMethod = () => {
			return () => {};
		};

		#connect: StandardConnectMethod = async (input) => {
			if (input?.silent) {
				await flow.restore();
				return { accounts: [] };
			}

			const url = await flow.startFlow(provider, clientId, redirectUrl, extraParams);

			const popup = window.open(url);

			if (!popup) {
				throw new Error('Unable to open sign-in window');
			}

			const redirectURL = new URL(redirectUrl);

			return new Promise((resolve, reject) => {
				const interval = setInterval(async () => {
					try {
						if (popup.closed) {
							clearInterval(interval);
							reject();
							return;
						}

						if (
							popup.location.origin === redirectURL.origin &&
							popup.location.pathname === redirectURL.pathname
						) {
							clearInterval(interval);
							popup.close();
							await flow.handleAuthRedirect(popup.location.hash);
							resolve({ accounts: [] });
						}
					} catch {
						// Ignore some errors that can happen due to cross-origin sniffing issues.
						// These _shouldn't_ matter in practice because when the page redirects, it'll
						// be back to same-origin.
					}
				}, 100);
			});
		};

		#signPersonalMessage: SuiSignPersonalMessageMethod = async () => {
			throw new Error('Not yet implemented');
		};

		#signTransactionBlock: SuiSignTransactionBlockMethod = async (transactionInput) => {
			const keypair = await flow.getKeypair();
			const { bytes, signature } = await transactionInput.transactionBlock.sign({
				client: flow.suiClient,
				signer: keypair,
			});

			return {
				transactionBlockBytes: bytes,
				signature: signature,
			};
		};

		#signAndExecuteTransactionBlock: SuiSignAndExecuteTransactionBlockMethod = async (
			transactionInput,
		) => {
			const keypair = await flow.getKeypair();
			return await flow.suiClient.signAndExecuteTransactionBlock({
				signer: keypair,
				transactionBlock: transactionInput.transactionBlock,
				options: transactionInput.options,
				requestType: transactionInput.requestType,
			});
		};
	}

	return wallets.register(new EnokiWallet());
}
