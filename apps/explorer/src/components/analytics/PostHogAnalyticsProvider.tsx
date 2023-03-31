// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { PostHogProvider } from 'posthog-js/react';
import { type ReactNode } from 'react';

import { GROWTHBOOK_FEATURES } from '~/utils/growthbook';

type PostHogProviderProps = {
    children: ReactNode;
};

export function PostHogAnalyticsProvider({ children }: PostHogProviderProps) {
    const { on: isEnabled } = useFeature(
        GROWTHBOOK_FEATURES.EXPLORER_POSTHOG_ANALYTICS
    );

    return isEnabled ? (
        <PostHogProvider
            apiKey="phc_IggVMJtR5vawlA4H3IIYnIyWjcK8rPiqAI1FlmKZPjp"
            options={{
                // We'll set memory as the default persistence method so that
                // we aren't required to show a cookie acceptance banner
                persistence: 'memory',
                // We need to manually collect page view events since we use client-side routing
                capture_pageview: false,
            }}
        >
            {children}
        </PostHogProvider>
    ) : (
        <>{children}</>
    );
}
