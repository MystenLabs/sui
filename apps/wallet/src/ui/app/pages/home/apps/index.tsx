// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { Route, Routes } from 'react-router-dom';

import { Content } from '_app/shared/bottom-menu-layout';
import FiltersPortal, { type Props } from '_components/filters-tags';
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
    const ecosystemApps =
        useFeature<DAppEntry[]>(FEATURES.WALLET_DAPPS).value ?? [];
    const uniqueAppTags = new Set<FilterTag>();

    ecosystemApps.forEach((app) => {
        app.tags.forEach((tag) => {
            uniqueAppTags.add({
                name: tag,
                link: `apps/?tagFilter=${tag.toLowerCase()}`,
            });
        });
    });

    const allFilterTags = [...defaultFilterTags, ...uniqueAppTags];

    return (
        <div className={st.container}>
            <Content>
                <section>
                    <FiltersPortal tags={allFilterTags} />
                    <Routes>
                        <Route path="/" element={<AppsPlayGround />} />
                        <Route
                            path="/connected"
                            element={<ConnectedAppsCard />}
                        />
                    </Routes>
                </section>
            </Content>
        </div>
    );
}

export default AppsPage;
