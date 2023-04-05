// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useState } from 'react';
import Browser from 'webextension-polyfill';

const IS_RATE_LIMITED_FROM_FAUCET_STORAGE_KEY = 'is_rate_limited_from_faucet';

const FAUCET_RATE_LIMIT_EXPIRY_TIME_STORAGE_KEY =
    'faucet_rate_limit_expiry_time';

const rateLimitExpiryTime = 24 * 60 * 60 * 1000;

export function useFaucetRateLimiter() {
    const [isRateLimited, setRateLimited] = useState(false);

    const rateLimit = useCallback(() => {
        Browser.storage.local.set({
            [IS_RATE_LIMITED_FROM_FAUCET_STORAGE_KEY]: true,
            [FAUCET_RATE_LIMIT_EXPIRY_TIME_STORAGE_KEY]:
                new Date().getTime() + rateLimitExpiryTime,
        });
    }, []);

    useEffect(() => {
        const changesCallback = (
            changes: Browser.Storage.StorageAreaOnChangedChangesType
        ) => {
            if (IS_RATE_LIMITED_FROM_FAUCET_STORAGE_KEY in changes) {
                const { newValue } =
                    changes[IS_RATE_LIMITED_FROM_FAUCET_STORAGE_KEY];

                setRateLimited(Boolean(newValue));
            }
        };

        Browser.storage.local.onChanged.addListener(changesCallback);
        return () => {
            Browser.storage.local.onChanged.removeListener(changesCallback);
        };
    }, []);

    useEffect(() => {
        Browser.storage.local
            .get({
                [FAUCET_RATE_LIMIT_EXPIRY_TIME_STORAGE_KEY]: null,
            })
            .then(
                ({
                    [FAUCET_RATE_LIMIT_EXPIRY_TIME_STORAGE_KEY]: expiryTime,
                }) => {
                    const currTime = new Date().getTime();
                    if (expiryTime && currTime > expiryTime) {
                        Browser.storage.local.set({
                            [IS_RATE_LIMITED_FROM_FAUCET_STORAGE_KEY]: false,
                            [FAUCET_RATE_LIMIT_EXPIRY_TIME_STORAGE_KEY]: null,
                        });
                        setRateLimited(false);
                    } else {
                        setRateLimited(true);
                    }
                }
            );
    }, []);

    return [isRateLimited, rateLimit] as const;
}
