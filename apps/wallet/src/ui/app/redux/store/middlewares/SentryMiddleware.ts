// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { reportSentryError } from '_src/shared/sentry';

import type { Middleware } from '@reduxjs/toolkit';

// Log event to Sentry via Redux toolkit middleware
export const SentryMiddleware: Middleware =
    ({ dispatch }) =>
    (next) =>
    (action) => {
        if (action.error) {
            reportSentryError(action.error);
        }

        return next(action);
    };
