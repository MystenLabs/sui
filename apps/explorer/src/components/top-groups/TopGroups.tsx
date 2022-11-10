// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RecentModulesCard } from '../../components/recent-modules-card/RecentModulesCard';
import { TopValidatorsCardAPI } from '../../components/top-validators-card/TopValidatorsCard';

import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

export function TopGroupsCard() {
    return (
        <TabGroup>
            <TabList>
                <Tab>Top Validators</Tab>
                <Tab>Recent Packages</Tab>
            </TabList>
            <TabPanels>
                <TabPanel>
                    <TopValidatorsCardAPI />
                </TabPanel>
                <TabPanel>
                    <RecentModulesCard />
                </TabPanel>
            </TabPanels>
        </TabGroup>
    );
}
