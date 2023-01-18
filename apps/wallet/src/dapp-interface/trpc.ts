import {
    type TRPCLink,
    TRPCClientError,
    createTRPCProxyClient,
} from '@trpc/client';
import { type AnyRouter } from '@trpc/server';
import { observable } from '@trpc/server/observable';

import { type AppRouter } from '_src/background/trpc';

// TODO: This doesn't really handle disconnection from the port, which can happen in the backend.
// We could entirely shield this from any concept of disconnecting, by making the port proxy just
// manage that for us.
const link: TRPCLink<AnyRouter> = (runtime) => {
    return ({ op }) => {
        return observable((observer) => {
            const listeners: (() => void)[] = [];

            const { id, type, path } = op;

            try {
                const input = runtime.transformer.serialize(op.input);

                const onMessage = (event: Event) => {
                    if (!(event instanceof CustomEvent)) return;
                    const message = event.detail;
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
                        } as any,
                    });

                    if (
                        type !== 'subscription' ||
                        trpc.result.type === 'stopped'
                    ) {
                        observer.complete();
                    }
                };

                window.addEventListener('trpc-response', onMessage);
                listeners.push(() =>
                    window.removeEventListener('trpc-response', onMessage)
                );

                window.dispatchEvent(
                    new CustomEvent('trpc-request', {
                        detail: {
                            trpc: {
                                id,
                                jsonrpc: undefined,
                                method: type,
                                params: { path, input },
                            },
                        },
                    })
                );
            } catch (cause) {
                observer.error(
                    new TRPCClientError(
                        cause instanceof Error ? cause.message : 'Unknown error'
                    )
                );
            }

            return () => {
                listeners.forEach((unsub) => unsub());
                if (type === 'subscription') {
                    window.dispatchEvent(
                        new CustomEvent('trpc-request', {
                            detail: {
                                trpc: {
                                    id,
                                    jsonrpc: undefined,
                                    method: 'subscription.stop',
                                },
                            },
                        })
                    );
                }
            };
        });
    };
};

export const trpc = createTRPCProxyClient<AppRouter>({
    links: [link],
});

trpc.ping.query().then(console.log, console.error);
trpc.onPing.subscribe(undefined, {
    onData(data) {
        console.log(data);
    },
});
