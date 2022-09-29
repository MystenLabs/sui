// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';

// TODO: Specify the type of the context
// Specify the type of the context
function TopGroupsCard() {
    // Mork data
    return (
        <TabGroup>
            <TabList>
                <Tab>Top Modules</Tab>
                <Tab>Top NFT Collections</Tab>
                <Tab>Top Addresses</Tab>
            </TabList>
            <TabPanels>
                <TabPanel>Top Modules Component</TabPanel>
                <TabPanel>Top NFT Collections Component</TabPanel>
                <TabPanel>Top Modules Component</TabPanel>
            </TabPanels>
        </TabGroup>
    );
}

export default TopGroupsCard;
