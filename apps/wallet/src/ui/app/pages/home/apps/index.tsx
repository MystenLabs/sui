// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { Route, Routes } from 'react-router-dom';

import { Content } from '_app/shared/bottom-menu-layout';
import FiltersPortal from '_components/filters-tags';
import AppsPlayGround, { ConnectedAppsCard } from '_components/sui-apps';
import { FEATURES } from '_src/shared/experimentation/features';

import type { DAppEntry } from '_src/ui/app/components/sui-apps/SuiApp';

import st from './AppsPage.module.scss';

type FilterTag = {
	name: string;
	link: string;
};

function AppsPage() {
	const defaultFilterTags: FilterTag[] = [
		{
			name: 'Connections',
			link: 'apps/connected',
		},
		{
			name: 'All',
			link: 'apps',
		},
	];
	const ecosystemApps = useFeature<DAppEntry[]>(FEATURES.WALLET_DAPPS).value ?? [];

	const uniqueAppTags = Array.from(new Set(ecosystemApps.flatMap((app) => app.tags))).map(
		(tag) => ({
			name: tag,
			// The tag subroute is used to get around the NavLink limitation with reading query params
			// Enables active route highlighting without excessive overhead
			link: `apps/${tag}`,
		}),
	);

	const allFilterTags = [...defaultFilterTags, ...uniqueAppTags];

	return (
		<div className={st.container}>
			<Content>
				<section>
					<FiltersPortal tags={allFilterTags} />
					<Routes>
						<Route path="/connected" element={<ConnectedAppsCard />} />
						<Route path="/:tagName?" element={<AppsPlayGround />} />
					</Routes>
				</section>
			</Content>
		</div>
	);
}

export default AppsPage;
