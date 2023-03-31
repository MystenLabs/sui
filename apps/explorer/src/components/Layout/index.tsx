// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { PostHogAnalyticsProvider } from '@mysten/core';

import { LayoutContent } from './LayoutContent';

import { growthbook, GROWTHBOOK_FEATURES } from '~/utils/growthbook';

export function Layout() {
    const isPostHogEnabled = growthbook.getFeatureValue(
        GROWTHBOOK_FEATURES.EXPLORER_POSTHOG_ANALYTICS,
        false
    );

    return (
        <PostHogAnalyticsProvider isEnabled={isPostHogEnabled}>
            <GrowthBookProvider growthbook={growthbook}>
                <LayoutContent />
            </GrowthBookProvider>
        </PostHogAnalyticsProvider>
    );
}
