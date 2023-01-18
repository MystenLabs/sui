// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type AnyProcedure,
    type AnyRouter,
    type ProcedureType,
    TRPCError,
} from '@trpc/server';
import { type Unsubscribable, isObservable } from '@trpc/server/observable';
import Browser from 'webextension-polyfill';

import { getErrorFromUnknown } from './errors';

import type { TRPCExtensionRequest, TRPCExtensionResponse } from './types';
import type { NodeHTTPCreateContextOption } from '@trpc/server/dist/adapters/node-http/types';
import type { BaseHandlerOptions } from '@trpc/server/dist/internals/types';

export type CreateExtensionContextOptions = {
    req: Browser.Runtime.Port;
    res: undefined;
};

export type CreateExtensionHandlerOptions<TRouter extends AnyRouter> = Pick<
    BaseHandlerOptions<TRouter, CreateExtensionContextOptions['req']> &
        NodeHTTPCreateContextOption<
            TRouter,
            CreateExtensionContextOptions['req'],
            CreateExtensionContextOptions['res']
        >,
    'router' | 'createContext' | 'onError'
>;

// Inspried by trpc-chrome: https://github.com/jlalmes/trpc-chrome
export const createExtensionHandler = <TRouter extends AnyRouter>(
    opts: CreateExtensionHandlerOptions<TRouter>
) => {
    const { router, createContext, onError } = opts;
    const { transformer } = router._def._config;

    Browser.runtime.onConnect.addListener((port) => {
        const subscriptions = new Map<number | string, Unsubscribable>();
        const listeners: (() => void)[] = [];

        const onDisconnect = () => {
            listeners.forEach((unsub) => unsub());
        };

        port.onDisconnect.addListener(onDisconnect);
        listeners.push(() => port.onDisconnect.removeListener(onDisconnect));

        const onMessage = async (message: TRPCExtensionRequest) => {
            if (!('trpc' in message)) return;
            const { trpc } = message;
            if (!('id' in trpc) || trpc.id === null || trpc.id === undefined)
                return;
            if (!trpc) return;

            const { id, jsonrpc, method } = trpc;

            const sendResponse = (response: TRPCExtensionResponse['trpc']) => {
                port.postMessage({
                    trpc: { id, jsonrpc, ...response },
                } as TRPCExtensionResponse);
            };

            let params: { path: string; input: unknown } | undefined;
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            let input: any;
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            let ctx: any;

            try {
                if (method === 'subscription.stop') {
                    const subscription = subscriptions.get(id);
                    if (subscription) {
                        subscription.unsubscribe();
                        sendResponse({
                            result: {
                                type: 'stopped',
                            },
                        });
                    }
                    subscriptions.delete(id);
                    return;
                }

                params = trpc.params;

                input = transformer.input.deserialize(params.input);

                ctx = await createContext?.({ req: port, res: undefined });
                const caller = router.createCaller(ctx);

                const segments = params.path.split('.');
                const procedureFn = segments.reduce(
                    (acc, segment) => acc[segment],
                    // eslint-disable-next-line @typescript-eslint/no-explicit-any
                    caller as any
                ) as AnyProcedure;

                const result = await procedureFn(input);

                if (method !== 'subscription') {
                    const data = transformer.output.serialize(result);
                    sendResponse({
                        result: {
                            type: 'data',
                            data,
                        },
                    });
                    return;
                }

                if (!isObservable(result)) {
                    throw new TRPCError({
                        message: `Subscription ${params.path} did not return an observable`,
                        code: 'INTERNAL_SERVER_ERROR',
                    });
                }

                const subscription = result.subscribe({
                    next: (data) => {
                        sendResponse({
                            result: {
                                type: 'data',
                                data,
                            },
                        });
                    },
                    error: (cause) => {
                        const error = getErrorFromUnknown(cause);

                        onError?.({
                            error,
                            type: method,
                            path: params?.path,
                            input,
                            ctx,
                            req: port,
                        });

                        sendResponse({
                            error: router.getErrorShape({
                                error,
                                type: method,
                                path: params?.path,
                                input,
                                ctx,
                            }),
                        });
                    },
                    complete: () => {
                        sendResponse({
                            result: {
                                type: 'stopped',
                            },
                        });
                    },
                });

                if (subscriptions.has(id)) {
                    subscription.unsubscribe();
                    sendResponse({
                        result: {
                            type: 'stopped',
                        },
                    });
                    throw new TRPCError({
                        message: `Duplicate id ${id}`,
                        code: 'BAD_REQUEST',
                    });
                }
                listeners.push(() => subscription.unsubscribe());

                subscriptions.set(id, subscription);

                sendResponse({
                    result: {
                        type: 'started',
                    },
                });
                return;
            } catch (cause) {
                const error = getErrorFromUnknown(cause);

                onError?.({
                    error,
                    type: method as ProcedureType,
                    path: params?.path,
                    input,
                    ctx,
                    req: port,
                });

                sendResponse({
                    error: router.getErrorShape({
                        error,
                        type: method as ProcedureType,
                        path: params?.path,
                        input,
                        ctx,
                    }),
                });
            }
        };

        port.onMessage.addListener(onMessage);
        listeners.push(() => port.onMessage.removeListener(onMessage));
    });
};
