// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    SUI_CHAINS,
    ReadonlyWalletAccount,
    type SuiSignAndExecuteTransactionFeature,
    type SuiSignAndExecuteTransactionMethod,
    type ConnectFeature,
    type ConnectMethod,
    type Wallet,
    type WalletEventNames,
    type WalletEvents,
} from '@mysten/wallet-standard';
import mitt, { type Emitter } from 'mitt';
import { filter, map, type Observable } from 'rxjs';

import icon from '../manifest/icons/sui-icon-128.png';
import { mapToPromise } from './utils';
import { createMessage } from '_messages';
import { WindowMessageStream } from '_messaging/WindowMessageStream';
import { type Payload } from '_payloads';
import {
    type AcquirePermissionsRequest,
    type AcquirePermissionsResponse,
    ALL_PERMISSION_TYPES,
} from '_payloads/permissions';

import type { GetAccount } from '_payloads/account/GetAccount';
import type { GetAccountResponse } from '_payloads/account/GetAccountResponse';
import type {
    ExecuteTransactionRequest,
    ExecuteTransactionResponse,
} from '_payloads/transactions';

type WalletEventsMap = {
    [E in WalletEventNames]: Parameters<WalletEvents[E]>[0];
};

// TODO: rebuild event interface with Mitt.
export class SuiWallet implements Wallet {
    readonly #events: Emitter<WalletEventsMap>;
    readonly #version = '1.0.0' as const;
    readonly #name = 'Sui Wallet' as const;
    #account: ReadonlyWalletAccount | null;
    #messagesStream: WindowMessageStream;

    get version() {
        return this.#version;
    }

    get name() {
        return this.#name;
    }

    get icon() {
        // TODO: Improve this with ideally a vector logo.
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        return icon as any;
    }

    get chains() {
        // TODO: Extract chain from wallet:
        return SUI_CHAINS;
    }

    get features(): ConnectFeature & SuiSignAndExecuteTransactionFeature {
        return {
            'standard:connect': {
                version: '1.0.0',
                connect: this.#connect,
            },
            'sui:signAndExecuteTransaction': {
                version: '1.0.0',
                signAndExecuteTransaction: this.#signAndExecuteTransaction,
            },
        };
    }

    get accounts() {
        return this.#account ? [this.#account] : [];
    }

    constructor() {
        this.#events = mitt();
        this.#account = null;
        this.#messagesStream = new WindowMessageStream(
            'sui_in-page',
            'sui_content-script'
        );

        this.#connected();
    }

    on<E extends WalletEventNames>(
        event: E,
        listener: WalletEvents[E]
    ): () => void {
        this.#events.on('standard:change', listener);
        return () => this.#events.off(event, listener);
    }

    #connected = async () => {
        const accounts = await mapToPromise(
            this.#send<GetAccount, GetAccountResponse>({
                type: 'get-account',
            }),
            (response) => response.accounts
        );

        const properties: ('accounts' | 'chains')[] = [];

        const [address] = accounts;

        if (address) {
            const account = this.#account;
            if (!account || account.address !== address) {
                this.#account = new ReadonlyWalletAccount({
                    address,
                    // TODO: Expose public key instead of address:
                    publicKey: new Uint8Array(),
                    chains: SUI_CHAINS,
                    features: [
                        'sui:signAndExecuteTransaction',
                        'standard:signMessage',
                    ],
                });
                properties.push('accounts');
            }
        }

        if (properties.length) {
            this.#events.emit('standard:change', properties);
        }
    };

    #connect: ConnectMethod = async (input) => {
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

        this.#connected();

        return { accounts: this.accounts };
    };

    #signAndExecuteTransaction: SuiSignAndExecuteTransactionMethod = async (
        input
    ) => {
        return mapToPromise(
            this.#send<ExecuteTransactionRequest, ExecuteTransactionResponse>({
                type: 'execute-transaction-request',
                transaction: {
                    type: 'v2',
                    data: input.transaction,
                },
            }),
            (response) => response.result
        );
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
