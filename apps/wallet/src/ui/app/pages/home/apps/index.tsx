// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Route, Routes } from 'react-router-dom';

import { Content } from '_app/shared/bottom-menu-layout';
import FiltersPortal from '_components/filters-tags';
import AppsPlayGround, { ConnectedAppsCard } from '_components/sui-apps';

import st from './AppsPage.module.scss';

function AppsPage() {
	const filterTags = [
		{
			name: 'Playground',
			link: 'apps',
		},
		{
			name: 'Active Connections',
			link: 'apps/connected',
		},
	];

	return (
		<div className={st.container}>
			<Content>
				<section>
					<FiltersPortal tags={filterTags} />
					<Routes>
						<Route path="/" element={<AppsPlayGround />} />
						<Route path="/connected" element={<ConnectedAppsCard />} />
					</Routes>
				</section>
			</Content>
		</div>
	);
}

export default AppsPage;
