import { initTRPC } from '@trpc/server';
import { observable } from '@trpc/server/observable';

import { createHandler } from './utils/trpcHandler';

const t = initTRPC.create({
    isServer: false,
    allowOutsideOfServer: true,
});

const appRouter = t.router({
    ping: t.procedure.query(() => 'pong'),
    onPing: t.procedure.subscription(() => {
        return observable<string>((emit) => {
            const interval = setInterval(() => {
                emit.next('hello');
            }, 1000);
            return () => {
                clearInterval(interval);
            };
        });
    }),
});

export type AppRouter = typeof appRouter;

createHandler(appRouter);
