// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { PostHogProvider } from 'posthog-js/react';
import { type ReactNode } from 'react';

type PostHogProviderProps = {
    projectApiKey: string;
    children: ReactNode;
};

export function PostHogAnalyticsProvider({
    projectApiKey,
    children,
}: PostHogProviderProps) {
    const { on: isEnabled } = useFeature('enable-posthog-analytics');

    return isEnabled ? (
        <PostHogProvider
            apiKey={projectApiKey}
            options={{
                // We'll set memory as the default persistence method so that
                // we aren't required to show a cookie acceptance banner
                persistence: 'memory',
                // We need to manually collect page view events since we use client-side routing
                capture_pageview: false,
                autocapture: false,
            }}
        >
            {children}
        </PostHogProvider>
    ) : (
        <>{children}</>
    );
}
