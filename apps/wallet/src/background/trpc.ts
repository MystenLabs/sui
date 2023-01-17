import { initTRPC } from '@trpc/server';
import { createHandler } from './utils/trpcHandler';

const t = initTRPC.create({
    isServer: false,
    allowOutsideOfServer: true,
});

const appRouter = t.router({
    ping: t.procedure.query(() => 'pong'),
});

export type AppRouter = typeof appRouter;

createHandler(appRouter);
