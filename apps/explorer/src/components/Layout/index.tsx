// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { PostHogAnalyticsProvider } from '@mysten/core';

import { LayoutContent } from './LayoutContent';

import { growthbook } from '~/utils/growthbook';

export function Layout() {
    return (
        <GrowthBookProvider growthbook={growthbook}>
            <PostHogAnalyticsProvider projectApiKey="phc_IggVMJtR5vawlA4H3IIYnIyWjcK8rPiqAI1FlmKZPjp">
                <LayoutContent />
            </PostHogAnalyticsProvider>
        </GrowthBookProvider>
    );
}
