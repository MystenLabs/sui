// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { GrowthBookProvider } from '@growthbook/growthbook-react';

import { PostHogAnalyticsProvider } from '../analytics/PostHogAnalyticsProvider';
import { LayoutContent } from './LayoutContent';

import { growthbook } from '~/utils/growthbook';

export function Layout() {
    return (
        <GrowthBookProvider growthbook={growthbook}>
            <PostHogAnalyticsProvider>
                <LayoutContent />
            </PostHogAnalyticsProvider>
        </GrowthBookProvider>
    );
}
