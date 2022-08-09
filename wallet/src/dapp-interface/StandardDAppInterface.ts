// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    CIPHER_DEFAULT,
    VERSION_1_0_0
} from '@solana/wallet-standard';
import { filter, lastValueFrom, map, take, type Observable } from 'rxjs';

import { WindowMessageStream } from '_messaging/WindowMessageStream';
import { isErrorPayload, type Payload } from '_payloads';
import { createMessage } from '_src/shared/messaging/messages';
import {
    ALL_PERMISSION_TYPES,
} from '_src/shared/messaging/messages/payloads/permissions';

import type {
    AllWalletAccountMethodNames,
    ConnectedAccount,
    ConnectInput,
    ConnectOutput,
    Wallet,
    WalletAccount,
    WalletAccountMethod,
    WalletEventNames,
    WalletEvents,

    SignAndSendTransactionInput,
    SignAndSendTransactionOutput,
    WalletAccountMethodNames} from '@solana/wallet-standard';
import type { GetAccount } from '_src/shared/messaging/messages/payloads/account/GetAccount';
import type { GetAccountResponse } from '_src/shared/messaging/messages/payloads/account/GetAccountResponse';
import type {
    AcquirePermissionsRequest,
    AcquirePermissionsResponse} from '_src/shared/messaging/messages/payloads/permissions';
import type {
    ExecuteTransactionRequest,
    ExecuteTransactionResponse,
} from '_src/shared/messaging/messages/payloads/transactions';

/** Sui Devnet */
export const CHAIN_SUI_DEVNET = 'sui:devnet';

/** Sui Localnet */
export const CHAIN_SUI_LOCALNET = 'sui:localnet';

function mapToPromise<T extends Payload, R>(
    stream: Observable<T>,
    project: (value: T) => R
) {
    return lastValueFrom(
        stream.pipe(
            take<T>(1),
            map<T, R>((response) => {
                if (isErrorPayload(response)) {
                    // TODO: throw proper error
                    throw new Error(response.message);
                }
                return project(response);
            })
        )
    );
}

const messageStream = new WindowMessageStream(
    'sui_in-page',
    'sui_content-script'
);

function send<
    RequestPayload extends Payload,
    ResponsePayload extends Payload | void = void
>(
    payload: RequestPayload,
    responseForID?: string
): Observable<ResponsePayload> {
    const msg = createMessage(payload, responseForID);
    messageStream.send(msg);
    return messageStream.messages.pipe(
        filter(({ id }) => id === msg.id),
        map((msg) => msg.payload as ResponsePayload)
    );
}

export class SuiWallet implements Wallet<SuiWalletAccount> {
    readonly name = 'Sui Wallet';
    readonly icon =
        'data:image/svg+xml;base64,PHN2ZyBjbGlwLXJ1bGU9ImV2ZW5vZGQiIGZpbGwtcnVsZT0iZXZlbm9kZCIgaW1hZ2UtcmVuZGVyaW5nPSJvcHRpbWl6ZVF1YWxpdHkiIHNoYXBlLXJlbmRlcmluZz0iZ2VvbWV0cmljUHJlY2lzaW9uIiB0ZXh0LXJlbmRlcmluZz0iZ2VvbWV0cmljUHJlY2lzaW9uIiB2aWV3Qm94PSIwIDAgNzg0LjM3IDEyNzcuMzkiIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyI+PGcgZmlsbC1ydWxlPSJub256ZXJvIj48cGF0aCBkPSJtMzkyLjA3IDAtOC41NyAyOS4xMXY4NDQuNjNsOC41NyA4LjU1IDM5Mi4wNi0yMzEuNzV6IiBmaWxsPSIjMzQzNDM0Ii8+PHBhdGggZD0ibTM5Mi4wNyAwLTM5Mi4wNyA2NTAuNTQgMzkyLjA3IDIzMS43NXYtNDA5Ljk2eiIgZmlsbD0iIzhjOGM4YyIvPjxwYXRoIGQ9Im0zOTIuMDcgOTU2LjUyLTQuODMgNS44OXYzMDAuODdsNC44MyAxNC4xIDM5Mi4zLTU1Mi40OXoiIGZpbGw9IiMzYzNjM2IiLz48cGF0aCBkPSJtMzkyLjA3IDEyNzcuMzh2LTMyMC44NmwtMzkyLjA3LTIzMS42M3oiIGZpbGw9IiM4YzhjOGMiLz48cGF0aCBkPSJtMzkyLjA3IDg4Mi4yOSAzOTIuMDYtMjMxLjc1LTM5Mi4wNi0xNzguMjF6IiBmaWxsPSIjMTQxNDE0Ii8+PHBhdGggZD0ibTAgNjUwLjU0IDM5Mi4wNyAyMzEuNzV2LTQwOS45NnoiIGZpbGw9IiMzOTM5MzkiLz48L2c+PC9zdmc+';
    readonly version = VERSION_1_0_0;

    // TODO: Figure out how to handle multi-environment chains:
    // @ts-expect-error: Types here are annoying
    readonly chains = [CHAIN_SUI_DEVNET];

    #listeners: { [E in WalletEventNames]?: WalletEvents[E][] } = {};
    #accounts: SuiWalletAccount[] = [];

    get accounts() {
        return [...this.#accounts];
    }

    get hasMoreAccounts() {
        return false;
    }

    get methods() {
        const methods = this.#accounts.flatMap((account) =>
            Object.keys(account.methods)
        ) as WalletAccountMethodNames<SuiWalletAccount>[];
        return [...new Set(methods)];
    }

    get ciphers() {
        return [];
    }

    async connect<
        Chain extends SuiWalletAccount['chain'],
        MethodNames extends WalletAccountMethodNames<SuiWalletAccount>,
        Input extends ConnectInput<SuiWalletAccount, Chain, MethodNames>
    >({
        chains,
        addresses,
        methods,
        silent,
    }: Input): Promise<
        ConnectOutput<SuiWalletAccount, Chain, MethodNames, Input>
    > {
        const permission = await mapToPromise(
            send<AcquirePermissionsRequest, AcquirePermissionsResponse>({
                type: 'acquire-permissions-request',
                // TODO: Granular permissioning, either based on methods, or some other standard:
                permissions: ALL_PERMISSION_TYPES,
            }),
            (response) => response.result
        );

        if (!permission) {
            // TODO: Improve error messaging:
            throw new Error('Denied permission.');
        }

        if (addresses) {
            // TODO: Either support this, or move it out of the standard.
            throw new Error(
                'Granular address-based connection is not supported.'
            );
        }

        const accounts = await mapToPromise(
            send<GetAccount, GetAccountResponse>({
                type: 'get-account',
            }),
            (response) => response.accounts
        );

        // TODO: Update internal accounts and emit event.
        const accountInstances = accounts.map(
            (address) =>
                new SuiWalletAccount({
                    address,
                    methods,
                })
        );

        return {
            // @ts-expect-error: Need to fix types
            accounts: accountInstances as ConnectedAccount<
                any,
                Chain,
                MethodNames,
                Input
            >[],
            // FIXME: this should be true if there are more accounts found for the given inputs that weren't granted access
            hasMoreAccounts: false,
        };
    }

    on<E extends WalletEventNames>(
        event: E,
        listener: WalletEvents[E]
    ): () => void {
        this.#listeners[event]?.push(listener) ||
            (this.#listeners[event] = [listener]);

        return (): void => this.#off(event, listener);
    }

    #emit<E extends WalletEventNames>(event: E): void {
        this.#listeners[event]?.forEach((listener) => listener());
    }

    #off<E extends WalletEventNames>(
        event: E,
        listener: WalletEvents[E]
    ): void {
        this.#listeners[event] = this.#listeners[event]?.filter(
            (existingListener) => listener !== existingListener
        );
    }
}

export type SuiWalletChain = typeof CHAIN_SUI_LOCALNET;

export class SuiWalletAccount implements WalletAccount {
    readonly chain = CHAIN_SUI_DEVNET;

    #address: string;

    get address() {
        return new TextEncoder().encode(this.#address);
    }

    get publicKey() {
        return this.address;
    }

    get ciphers() {
        return [CIPHER_DEFAULT];
    }

    // TODO: Implement access control via permissioning here:
    get methods(): WalletAccountMethod<this> {
        return {
            signAndSendTransaction: (...args) =>
                this.#signAndSendTransaction(...args),
        };
    }

    constructor({
        address,
    }: {
        address: string;
        methods?: AllWalletAccountMethodNames<SuiWalletAccount>[];
    }) {
        this.#address = address;
    }

    async #signAndSendTransaction(
        input: SignAndSendTransactionInput<this>
    ): Promise<SignAndSendTransactionOutput<this>> {
        if (input.extraSigners?.length) {
            throw new Error('Extra signers are not supported.');
        }

        const signatures: Uint8Array[] = [];

        for (const transactionBytes of input.transactions) {
            const response = await mapToPromise(
                send<ExecuteTransactionRequest, ExecuteTransactionResponse>({
                    type: 'execute-transaction-request',
                    transactionBytes,
                }),
                (response) => response.result
            );

            const [data] = Object.values(response)[0];

            signatures.push(data.certificate.transactionDigest);
        }

        // TODO: The standard wants you to return a `signature`, but we should return the usable part of the response,
        // which is the digest. Ideally, we'd provide the entire response rather than this small subset too.
        return { signatures };
    }
}
