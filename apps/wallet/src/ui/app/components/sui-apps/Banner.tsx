// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import ExternalLink from '../external-link';
import { ampli } from '_src/shared/analytics/ampli';
import { FEATURES } from '_src/shared/experimentation/features';

export type BannerProps = {
	enabled: boolean;
	bannerUrl?: string;
	imageUrl?: string;
};

export function AppsPageBanner() {
	const AppsBannerConfig = useFeature<BannerProps>(FEATURES.WALLET_APPS_BANNER_CONFIG);

	if (!AppsBannerConfig.value?.enabled) {
		return null;
	}

	return (
		<div className="mb-3">
			{AppsBannerConfig.value?.bannerUrl && (
				<ExternalLink
					href={AppsBannerConfig.value?.bannerUrl}
					onClick={() => ampli.clickedBullsharkQuestsCta({ sourceFlow: 'Banner - Apps tab' })}
				>
					<img src={AppsBannerConfig.value?.imageUrl} alt="Apps Banner" />
				</ExternalLink>
			)}
		</div>
	);
}
