// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Sentry from '@sentry/react';
import { BrowserTracing } from '@sentry/tracing';
import { useEffect } from 'react';
import {
    createRoutesFromChildren,
    matchRoutes,
    useLocation,
    useNavigationType,
} from 'react-router-dom';

import { featuresPromise, growthbook } from './growthbook';

Sentry.init({
    enabled: import.meta.env.PROD,
    dsn: 'https://e4251274d1b141d7ba272103fa0f8d83@o1314142.ingest.sentry.io/6564988',
    environment: import.meta.env.VITE_VERCEL_ENV,
    integrations: [
        new BrowserTracing({
            routingInstrumentation: Sentry.reactRouterV6Instrumentation(
                useEffect,
                useLocation,
                useNavigationType,
                createRoutesFromChildren,
                matchRoutes
            ),
        }),
    ],
    // NOTE: Even though this is set to 1, we actually will properly sample the event in `beforeSendTransaction`.
    // We don't do sampling here or in `tracesSampler` because those can't be async, so we can't wait for
    // the features from growthbook to load before applying sampling.
    tracesSampleRate: 1,
    async beforeSendTransaction(event) {
        await featuresPromise;
        const sampleRate = growthbook.getFeatureValue(
            'explorer-sentry-tracing',
            0
        );
        if (sampleRate > Math.random()) {
            return event;
        } else {
            return null;
        }
    },
    beforeSend(event) {
        try {
            // Filter out any code from unknown sources:
            if (
                !event.exception?.values?.[0].stacktrace ||
                event.exception?.values?.[0].stacktrace?.frames?.[0]
                    .filename === '<anonymous>'
            ) {
                return null;
            }
        } catch (e) {}

        return event;
    },

    denyUrls: [
        // Chrome extensions
        /extensions\//i,
				/^chrome(?:-extension)?:\/\//i,
        /<anonymous>/,
    ],
    allowUrls: [
        /.*\.sui\.io/i,
        /.*-mysten-labs\.vercel\.app/i,
        'explorer-topaz.vercel.app',
    ],
});
