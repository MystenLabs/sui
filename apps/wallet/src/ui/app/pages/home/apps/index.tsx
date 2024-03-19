// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Content } from '_app/shared/bottom-menu-layout';
import FiltersPortal from '_components/filters-tags';
import AppsPlayGround, { ConnectedAppsCard } from '_components/sui-apps';
import { getFromSessionStorage, setToSessionStorage } from '_src/background/storage-utils';
import { FEATURES } from '_src/shared/experimentation/features';
import type { DAppEntry } from '_src/ui/app/components/sui-apps/SuiApp';
import { useUnlockedGuard } from '_src/ui/app/hooks/useUnlockedGuard';
import { useFeature } from '@growthbook/growthbook-react';
import { useEffect } from 'react';
import { Route, Routes, useNavigate } from 'react-router-dom';

import st from './AppsPage.module.scss';

const APPS_PAGE_NAVIGATION = 'APPS_PAGE_NAVIGATION';

type FilterTag = {
	name: string;
	link: string;
};

function AppsPage() {
	const navigate = useNavigate();

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

	const uniqueAppTags = Array.from(new Set(ecosystemApps.flatMap((app) => app.tags)))
		.map((tag) => ({
			name: tag,
			// The tag subroute is used to get around the NavLink limitation with reading query params
			// Enables active route highlighting without excessive overhead
			link: `apps/${tag}`,
		}))
		.sort((a, b) => a.name.localeCompare(b.name));

	const allFilterTags = [...defaultFilterTags, ...uniqueAppTags];

	useEffect(() => {
		getFromSessionStorage<string>(APPS_PAGE_NAVIGATION).then((activeTagLink) => {
			if (activeTagLink) {
				navigate(`/${activeTagLink}`);

				const element = document.getElementById(activeTagLink);

				if (element) {
					element.scrollIntoView();
				}
			}
		});
	}, [navigate]);

	const handleFiltersPortalClick = async (tag: FilterTag) => {
		await setToSessionStorage<string>(APPS_PAGE_NAVIGATION, tag.link);
	};

	if (useUnlockedGuard()) {
		return null;
	}

	return (
		<div className={st.container} data-testid="apps-page">
			<Content>
				<section>
					<FiltersPortal firstLastMargin tags={allFilterTags} callback={handleFiltersPortalClick} />
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
