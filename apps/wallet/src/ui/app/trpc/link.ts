// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TRPCClientError, type TRPCLink } from '@trpc/client';
import { observable } from '@trpc/server/observable';
import Browser from 'webextension-polyfill';

import type {
    TRPCExtensionRequest,
    TRPCExtensionResponse,
} from '../../../background/trpc/adapter/types';
import type { AnyRouter } from '@trpc/server';

// TODO: This should re-subscribe to subscriptions once the port disconnects.
export function backgroundLink<TRouter extends AnyRouter>(): TRPCLink<TRouter> {
    // TODO: For non-subscriptions, we could avoid using persistent connections and just
    // use the request / response model. That might be preferable because we don't
    // need to worry about the port disconnecting during the application lifecycle.
    const port = Browser.runtime.connect({ name: 'trpc' });

    return (runtime) => {
        return ({ op }) => {
            return observable((observer) => {
                const listeners: (() => void)[] = [];

                const { id, type, path } = op;

                try {
                    const input = runtime.transformer.serialize(op.input);

                    const onDisconnect = () => {
                        observer.error(
                            new TRPCClientError('Port disconnected prematurely')
                        );
                    };

                    port.onDisconnect.addListener(onDisconnect);
                    listeners.push(() =>
                        port.onDisconnect.removeListener(onDisconnect)
                    );

                    const onMessage = (message: TRPCExtensionResponse) => {
                        if (!('trpc' in message)) return;
                        const { trpc } = message;
                        if (!trpc) return;
                        if (
                            !('id' in trpc) ||
                            trpc.id === null ||
                            trpc.id === undefined
                        )
                            return;
                        if (id !== trpc.id) return;

                        if ('error' in trpc) {
                            const error = runtime.transformer.deserialize(
                                trpc.error
                            );
                            observer.error(
                                TRPCClientError.from({ ...trpc, error })
                            );
                            return;
                        }

                        observer.next({
                            result: {
                                ...trpc.result,
                                ...((!trpc.result.type ||
                                    trpc.result.type === 'data') && {
                                    type: 'data',
                                    data: runtime.transformer.deserialize(
                                        trpc.result.data
                                    ),
                                }),
                                // eslint-disable-next-line @typescript-eslint/no-explicit-any
                            } as any,
                        });

                        if (
                            type !== 'subscription' ||
                            trpc.result.type === 'stopped'
                        ) {
                            observer.complete();
                        }
                    };

                    port.onMessage.addListener(onMessage);
                    listeners.push(() =>
                        port.onMessage.removeListener(onMessage)
                    );

                    port.postMessage({
                        trpc: {
                            id,
                            jsonrpc: undefined,
                            method: type,
                            params: { path, input },
                        },
                    } as TRPCExtensionRequest);
                } catch (cause) {
                    observer.error(
                        new TRPCClientError(
                            cause instanceof Error
                                ? cause.message
                                : 'Unknown error'
                        )
                    );
                }

                return () => {
                    listeners.forEach((unsub) => unsub());
                    if (type === 'subscription') {
                        port.postMessage({
                            trpc: {
                                id,
                                jsonrpc: undefined,
                                method: 'subscription.stop',
                            },
                        } as TRPCExtensionRequest);
                    }
                };
            });
        };
    };
}
