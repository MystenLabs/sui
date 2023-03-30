// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBookProvider } from '@growthbook/growthbook-react';
import { PostHogAnalyticsProvider } from '@mysten/core';

import { Layout } from './Layout';

import { growthbook, GROWTHBOOK_FEATURES } from '~/utils/growthbook';

export function LayoutContainer() {
    const isPostHogEnabled = growthbook.getFeatureValue(
        GROWTHBOOK_FEATURES.EXPLORER_POSTHOG_ANALYTICS,
        false
    );

    return (
        <PostHogAnalyticsProvider isEnabled>
            <GrowthBookProvider growthbook={growthbook}>
                <Layout />
            </GrowthBookProvider>
        </PostHogAnalyticsProvider>
    );
}
