// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    GrowthBookProvider,
    useFeature,
} from '@growthbook/growthbook-react';
import { PostHogAnalyticsProvider } from '@mysten/core';

import { LayoutContent } from './LayoutContent';

import { growthbook, GROWTHBOOK_FEATURES } from '~/utils/growthbook';

export function Layout() {
    return (
        <GrowthBookProvider growthbook={growthbook}>
            <WithPostHogMaybeEnabled>
                <LayoutContent />
            </WithPostHogMaybeEnabled>
        </GrowthBookProvider>
    );
}

type WithPostHogEnabledProps = {
    children: React.ReactNode;
};

function WithPostHogMaybeEnabled({ children }: WithPostHogEnabledProps) {
    const { on: isEnabled } = useFeature(
        GROWTHBOOK_FEATURES.EXPLORER_POSTHOG_ANALYTICS
    );
    return (
        <PostHogAnalyticsProvider isEnabled={isEnabled}>
            {children}
        </PostHogAnalyticsProvider>
    );
}
