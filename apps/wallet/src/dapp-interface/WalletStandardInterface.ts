// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiAddress, toB64, TransactionBlock } from '@mysten/sui.js';
import {
    SUI_CHAINS,
    ReadonlyWalletAccount,
    SUI_DEVNET_CHAIN,
    SUI_TESTNET_CHAIN,
    SUI_LOCALNET_CHAIN,
    type SuiFeatures,
    type SuiSignAndExecuteTransactionBlockMethod,
    type StandardConnectFeature,
    type StandardConnectMethod,
    type Wallet,
    type StandardEventsFeature,
    type StandardEventsOnMethod,
    type StandardEventsListeners,
    type SuiSignTransactionBlockMethod,
    type SuiSignMessageMethod,
    SUI_MAINNET_CHAIN,
} from '@mysten/wallet-standard';
import mitt, { type Emitter } from 'mitt';
import { filter, map, type Observable } from 'rxjs';

import { mapToPromise } from './utils';
import { createMessage } from '_messages';
import { WindowMessageStream } from '_messaging/WindowMessageStream';
import {
    type AcquirePermissionsRequest,
    type AcquirePermissionsResponse,
    type HasPermissionsRequest,
    type HasPermissionsResponse,
    ALL_PERMISSION_TYPES,
} from '_payloads/permissions';
import { API_ENV } from '_src/shared/api-env';
import {
    isQredoConnectPayload,
    type QredoConnectPayload,
} from '_src/shared/messaging/messages/payloads/QredoConnect';
import { type SignMessageRequest } from '_src/shared/messaging/messages/payloads/transactions/SignMessage';
import { isWalletStatusChangePayload } from '_src/shared/messaging/messages/payloads/wallet-status-change';

import type { BasePayload, Payload } from '_payloads';
import type { GetAccount } from '_payloads/account/GetAccount';
import type { GetAccountResponse } from '_payloads/account/GetAccountResponse';
import type { SetNetworkPayload } from '_payloads/network';
import type {
    StakeRequest,
    ExecuteTransactionRequest,
    ExecuteTransactionResponse,
    SignTransactionRequest,
    SignTransactionResponse,
} from '_payloads/transactions';
import type { NetworkEnvType } from '_src/background/NetworkEnv';

type WalletEventsMap = {
    [E in keyof StandardEventsListeners]: Parameters<
        StandardEventsListeners[E]
    >[0];
};

// NOTE: Because this runs in a content script, we can't fetch the manifest.
const name = process.env.APP_NAME || 'Sui Wallet';

type StakeInput = { validatorAddress: string };
type SuiWalletStakeFeature = {
    'suiWallet:stake': {
        version: '0.0.1';
        stake: (input: StakeInput) => Promise<void>;
    };
};
export type QredoConnectInput = {
    service: string;
    apiUrl: string;
    token: string;
};
type QredoConnectFeature = {
    'qredo:connect': {
        version: '0.0.1';
        qredoConnect: (input: QredoConnectInput) => Promise<void>;
    };
};
type ChainType = Wallet['chains'][number];
const API_ENV_TO_CHAIN: Record<
    Exclude<API_ENV, API_ENV.customRPC>,
    ChainType
> = {
    [API_ENV.local]: SUI_LOCALNET_CHAIN,
    [API_ENV.devNet]: SUI_DEVNET_CHAIN,
    [API_ENV.testNet]: SUI_TESTNET_CHAIN,
    [API_ENV.mainnet]: SUI_MAINNET_CHAIN,
};

export class SuiWallet implements Wallet {
    readonly #events: Emitter<WalletEventsMap>;
    readonly #version = '1.0.0' as const;
    readonly #name = name;
    #accounts: ReadonlyWalletAccount[];
    #messagesStream: WindowMessageStream;
    #activeChain: ChainType | null = null;

    get version() {
        return this.#version;
    }

    get name() {
        return this.#name;
    }

    get icon() {
        return 'data:image/svg+xml;base64,PHN2ZyB3aWR0aD0iNzIiIGhlaWdodD0iNzIiIHZpZXdCb3g9IjAgMCA3MiA3MiIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj4KPHJlY3Qgd2lkdGg9IjcyIiBoZWlnaHQ9IjcyIiByeD0iMTYiIGZpbGw9IiM2RkJDRjAiLz4KPHBhdGggZmlsbC1ydWxlPSJldmVub2RkIiBjbGlwLXJ1bGU9ImV2ZW5vZGQiIGQ9Ik0yMC40MjEzIDUyLjc4MzhDMjMuNjQ5NiA1OC4zNzYgMjkuNDMyMSA2MS43MTQyIDM1Ljg4ODggNjEuNzE0MkM0Mi4zNDU1IDYxLjcxNDIgNDguMTI3IDU4LjM3NiA1MS4zNTY0IDUyLjc4MzhDNTQuNTg0OCA0Ny4xOTI2IDU0LjU4NDggNDAuNTE2MyA1MS4zNTY0IDM0LjkyNEwzNy43NTI0IDExLjM2MTVDMzYuOTI0MSA5LjkyNzAxIDM0Ljg1MzUgOS45MjcwMSAzNC4wMjUzIDExLjM2MTVMMjAuNDIxMyAzNC45MjRDMTcuMTkyOSA0MC41MTUyIDE3LjE5MjkgNDcuMTkxNSAyMC40MjEzIDUyLjc4MzhaTTMyLjA1NjYgMjIuNTcxM0wzNC45NTcxIDE3LjU0NzRDMzUuMzcxMiAxNi44MzAxIDM2LjQwNjUgMTYuODMwMSAzNi44MjA2IDE3LjU0NzRMNDcuOTc5MSAzNi44NzQ4QzUwLjAyOTEgNDAuNDI1NCA1MC40MTM5IDQ0LjUzNSA0OS4xMzM1IDQ4LjI5NTRDNDkuMDAwMiA0Ny42ODE5IDQ4LjgxMzggNDcuMDU0MiA0OC41NjI2IDQ2LjQyMDFDNDcuMDIxMyA0Mi41MzA0IDQzLjUzNjMgMzkuNTI4OSAzOC4yMDIzIDM3LjQ5ODJDMzQuNTM1MSAzNi4xMDcxIDMyLjE5NDMgMzQuMDYxMyAzMS4yNDMxIDMxLjQxNzFDMzAuMDE4IDI4LjAwODkgMzEuMjk3NiAyNC4yOTI0IDMyLjA1NjYgMjIuNTcxM1pNMjcuMTEwNyAzMS4xMzc5TDIzLjc5ODYgMzYuODc0OEMyMS4yNzQ4IDQxLjI0NTkgMjEuMjc0OCA0Ni40NjQxIDIzLjc5ODYgNTAuODM1M0MyNi4zMjIzIDU1LjIwNjQgMzAuODQxMyA1Ny44MTUgMzUuODg4OCA1Ny44MTVDMzkuMjQxMyA1Ny44MTUgNDIuMzYxNSA1Ni42NjMzIDQ0LjgxODQgNTQuNjA4OEM0NS4xMzg4IDUzLjgwMjEgNDYuMTMxIDUwLjg0OTIgNDQuOTA1MiA0Ny44MDU4QzQzLjc3MyA0NC45OTU0IDQxLjA0ODIgNDIuNzUxOSAzNi44MDYxIDQxLjEzNkMzMi4wMTEgMzkuMzE3MSAyOC44OTU4IDM2LjQ3NzQgMjcuNTQ4NiAzMi42OTg0QzI3LjM2MzEgMzIuMTc4MSAyNy4yMTg5IDMxLjY1NjggMjcuMTEwNyAzMS4xMzc5WiIgZmlsbD0id2hpdGUiLz4KPC9zdmc+' as const;
    }

    get chains() {
        // TODO: Extract chain from wallet:
        return SUI_CHAINS;
    }

    get features(): StandardConnectFeature &
        StandardEventsFeature &
        SuiFeatures &
        SuiWalletStakeFeature &
        QredoConnectFeature {
        return {
            'standard:connect': {
                version: '1.0.0',
                connect: this.#connect,
            },
            'standard:events': {
                version: '1.0.0',
                on: this.#on,
            },
            'sui:signTransactionBlock': {
                version: '1.0.0',
                signTransactionBlock: this.#signTransactionBlock,
            },
            'sui:signAndExecuteTransactionBlock': {
                version: '1.0.0',
                signAndExecuteTransactionBlock:
                    this.#signAndExecuteTransactionBlock,
            },
            'suiWallet:stake': {
                version: '0.0.1',
                stake: this.#stake,
            },
            'sui:signMessage': {
                version: '1.0.0',
                signMessage: this.#signMessage,
            },
            'qredo:connect': {
                version: '0.0.1',
                qredoConnect: this.#qredoConnect,
            },
        };
    }

    get accounts() {
        return this.#accounts;
    }

    #setAccounts(addresses: SuiAddress[]) {
        this.#accounts = addresses.map(
            (address) =>
                new ReadonlyWalletAccount({
                    address,
                    // TODO: Expose public key instead of address:
                    publicKey: new Uint8Array(),
                    chains: this.#activeChain ? [this.#activeChain] : [],
                    features: ['sui:signAndExecuteTransaction'],
                })
        );
    }

    constructor() {
        this.#events = mitt();
        this.#accounts = [];
        this.#messagesStream = new WindowMessageStream(
            'sui_in-page',
            'sui_content-script'
        );
        this.#messagesStream.messages.subscribe(({ payload }) => {
            if (isWalletStatusChangePayload(payload)) {
                const { network, accounts } = payload;
                if (network) {
                    this.#setActiveChain(network);
                    if (!accounts) {
                        // in case an accounts change exists skip updating chains of current accounts
                        // accounts will be updated in the if block below
                        this.#accounts = this.#accounts.map(
                            ({ address, features, icon, label, publicKey }) =>
                                new ReadonlyWalletAccount({
                                    address,
                                    publicKey,
                                    chains: this.#activeChain
                                        ? [this.#activeChain]
                                        : [],
                                    features,
                                    label,
                                    icon,
                                })
                        );
                    }
                }
                if (accounts) {
                    this.#setAccounts(accounts);
                }
                this.#events.emit('change', { accounts: this.accounts });
            }
        });
        this.#connected();
    }

    #on: StandardEventsOnMethod = (event, listener) => {
        this.#events.on(event, listener);
        return () => this.#events.off(event, listener);
    };

    #connected = async () => {
        this.#setActiveChain(await this.#getActiveNetwork());
        if (!(await this.#hasPermissions(['viewAccount']))) {
            return;
        }
        const accounts = await this.#getAccounts();
        this.#setAccounts(accounts);
        if (this.#accounts.length) {
            this.#events.emit('change', { accounts: this.accounts });
        }
    };

    #connect: StandardConnectMethod = async (input) => {
        if (!input?.silent) {
            await mapToPromise(
                this.#send<
                    AcquirePermissionsRequest,
                    AcquirePermissionsResponse
                >({
                    type: 'acquire-permissions-request',
                    permissions: ALL_PERMISSION_TYPES,
                }),
                (response) => response.result
            );
        }

        await this.#connected();

        return { accounts: this.accounts };
    };

    #signTransactionBlock: SuiSignTransactionBlockMethod = async (input) => {
        if (!TransactionBlock.is(input.transactionBlock)) {
            throw new Error(
                'Unexpect transaction format found. Ensure that you are using the `Transaction` class.'
            );
        }

        return mapToPromise(
            this.#send<SignTransactionRequest, SignTransactionResponse>({
                type: 'sign-transaction-request',
                transaction: {
                    ...input,
                    // account might be undefined if previous version of adapters is used
                    // in that case use the first account address
                    account:
                        input.account?.address ||
                        this.#accounts[0]?.address ||
                        '',
                    transaction: input.transactionBlock.serialize(),
                },
            }),
            (response) => response.result
        );
    };

    #signAndExecuteTransactionBlock: SuiSignAndExecuteTransactionBlockMethod =
        async (input) => {
            if (!TransactionBlock.is(input.transactionBlock)) {
                throw new Error(
                    'Unexpect transaction format found. Ensure that you are using the `Transaction` class.'
                );
            }

            return mapToPromise(
                this.#send<
                    ExecuteTransactionRequest,
                    ExecuteTransactionResponse
                >({
                    type: 'execute-transaction-request',
                    transaction: {
                        type: 'transaction',
                        data: input.transactionBlock.serialize(),
                        options: input.options,
                        // account might be undefined if previous version of adapters is used
                        // in that case use the first account address
                        account:
                            input.account?.address ||
                            this.#accounts[0]?.address ||
                            '',
                    },
                }),
                (response) => response.result
            );
        };

    #stake = async (input: StakeInput) => {
        this.#send<StakeRequest, void>({
            type: 'stake-request',
            validatorAddress: input.validatorAddress,
        });
    };

    #signMessage: SuiSignMessageMethod = async ({ message, account }) => {
        return mapToPromise(
            this.#send<SignMessageRequest, SignMessageRequest>({
                type: 'sign-message-request',
                args: {
                    message: toB64(message),
                    accountAddress: account.address,
                },
            }),
            (response) => {
                if (!response.return) {
                    throw new Error('Invalid sign message response');
                }
                return response.return;
            }
        );
    };

    #hasPermissions(permissions: HasPermissionsRequest['permissions']) {
        return mapToPromise(
            this.#send<HasPermissionsRequest, HasPermissionsResponse>({
                type: 'has-permissions-request',
                permissions: permissions,
            }),
            ({ result }) => result
        );
    }

    #getAccounts() {
        return mapToPromise(
            this.#send<GetAccount, GetAccountResponse>({
                type: 'get-account',
            }),
            (response) => response.accounts
        );
    }

    #getActiveNetwork() {
        return mapToPromise(
            this.#send<BasePayload, SetNetworkPayload>({
                type: 'get-network',
            }),
            ({ network }) => network
        );
    }

    #setActiveChain({ env }: NetworkEnvType) {
        this.#activeChain =
            env === API_ENV.customRPC ? 'sui:unknown' : API_ENV_TO_CHAIN[env];
    }

    #qredoConnect = async (input: QredoConnectInput): Promise<void> => {
        const allowed = await mapToPromise(
            this.#send<
                QredoConnectPayload<'connect'>,
                QredoConnectPayload<'connectResponse'>
            >({
                type: 'qredo-connect',
                method: 'connect',
                args: { ...input },
            }),
            (response) => {
                if (!isQredoConnectPayload(response, 'connectResponse')) {
                    throw new Error('Invalid qredo connect response');
                }
                return response.args.allowed;
            }
        );
        if (!allowed) {
            throw new Error('Rejected by user');
        }
    };

    #send<
        RequestPayload extends Payload,
        ResponsePayload extends Payload | void = void
    >(
        payload: RequestPayload,
        responseForID?: string
    ): Observable<ResponsePayload> {
        const msg = createMessage(payload, responseForID);
        this.#messagesStream.send(msg);
        return this.#messagesStream.messages.pipe(
            filter(({ id }) => id === msg.id),
            map((msg) => msg.payload as ResponsePayload)
        );
    }
}
