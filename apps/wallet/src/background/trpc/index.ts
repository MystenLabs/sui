// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { initTRPC } from '@trpc/server';
import { observable } from '@trpc/server/observable';

import { createExtensionHandler } from './adapter';

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

export function setupTRPC() {
    createExtensionHandler({ router: appRouter });
}
