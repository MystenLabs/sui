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
import { useMemo } from 'react';

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

    const uniqueAppTags = useMemo(() => {
        const tagsSet = new Set<FilterTag>();

        const flattenedTags = ecosystemApps.flatMap((app) =>
            app.tags.map((tag) => ({
                name: tag,
                link: `apps/?tagFilter=${tag.toLowerCase()}`,
            }))
        );

        flattenedTags.forEach((tag) => {
            tagsSet.add(tag);
        });

        return [...tagsSet];
    }, [ecosystemApps]);

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
