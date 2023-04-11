// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';

const FAUCET_RATE_LIMIT_EXPIRY_TIME_KEY = 'faucet_rate_limit_expiry_time';

// We'll rate limit users for 20 minutes which should
// more-or-less mock how the faucet API rate limits users
const rateLimitExpiryTime = 20 * 60 * 1000;

export function useFaucetRateLimiter() {
    const [isRateLimited, setRateLimited] = useState(() => {
        const expiryTime = localStorage.getItem(
            FAUCET_RATE_LIMIT_EXPIRY_TIME_KEY
        );
        return Date.now() <= Number(expiryTime);
    });

    const rateLimit = () => {
        const expiryTime = Date.now() + rateLimitExpiryTime;

        localStorage.setItem(
            FAUCET_RATE_LIMIT_EXPIRY_TIME_KEY,
            String(expiryTime)
        );
        setRateLimited(true);
    };

    useEffect(() => {
        if (!isRateLimited) {
            localStorage.removeItem(FAUCET_RATE_LIMIT_EXPIRY_TIME_KEY);
        }
    }, [isRateLimited]);

    return [isRateLimited, rateLimit] as const;
}
