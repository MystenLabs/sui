import {
    TRPCError,
    type AnyProcedure,
    type AnyRouter,
    type ProcedureType,
} from '@trpc/server';
import { type Unsubscribable, isObservable } from '@trpc/server/observable';
import {
    type TRPCClientOutgoingMessage,
    type TRPCErrorResponse,
    type TRPCRequest,
    type TRPCResultMessage,
} from '@trpc/server/rpc';
import { runtime } from 'webextension-polyfill';

export type TRPCChromeRequest = {
    trpc: TRPCRequest | TRPCClientOutgoingMessage;
};

export type TRPCChromeSuccessResponse = {
    trpc: TRPCResultMessage<any>;
};

export type TRPCChromeErrorResponse = {
    trpc: TRPCErrorResponse;
};

export type TRPCChromeResponse =
    | TRPCChromeSuccessResponse
    | TRPCChromeErrorResponse;

function getErrorFromUnknown(cause: unknown): TRPCError {
    if (cause instanceof Error && cause.name === 'TRPCError') {
        return cause as TRPCError;
    }

    let errorCause: Error | undefined = undefined;
    let stack: string | undefined = undefined;

    if (cause instanceof Error) {
        errorCause = cause;
        stack = cause.stack;
    }

    const error = new TRPCError({
        message: 'Internal server error',
        code: 'INTERNAL_SERVER_ERROR',
        cause: errorCause,
    });

    if (stack) {
        error.stack = stack;
    }

    return error;
}

export const createHandler = <TRouter extends AnyRouter>(router: TRouter) => {
    const { transformer } = router._def._config;

    runtime.onConnect.addListener((port) => {
        const subscriptions = new Map<number | string, Unsubscribable>();
        const listeners: (() => void)[] = [];

        const onDisconnect = () => {
            listeners.forEach((unsub) => unsub());
        };

        port.onDisconnect.addListener(onDisconnect);
        listeners.push(() => port.onDisconnect.removeListener(onDisconnect));

        const onMessage = async (message: TRPCChromeRequest) => {
            if (!('trpc' in message)) return;
            const { trpc } = message;
            if (!('id' in trpc) || trpc.id === null || trpc.id === undefined)
                return;
            if (!trpc) return;

            const { id, jsonrpc, method } = trpc;

            const sendResponse = (response: TRPCChromeResponse['trpc']) => {
                port.postMessage({
                    trpc: { id, jsonrpc, ...response },
                } as TRPCChromeResponse);
            };

            let params: { path: string; input: unknown } | undefined;
            let input: any;
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

                // TODO: Context
                ctx = undefined;
                const caller = router.createCaller(ctx);

                const segments = params.path.split('.');
                const procedureFn = segments.reduce(
                    (acc, segment) => acc[segment],
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
                        message:
                            'Subscription ${params.path} did not return an observable',
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
