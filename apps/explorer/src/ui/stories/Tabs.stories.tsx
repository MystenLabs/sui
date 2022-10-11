// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { TabGroup, TabList, Tab, TabPanels, TabPanel } from '../Tabs';

import type { ComponentMeta, ComponentStory } from '@storybook/react';

export default {
    title: 'UI/Tabs',
    component: TabGroup,
} as ComponentMeta<typeof TabGroup>;

const Template: ComponentStory<typeof TabGroup> = (args) => (
    <TabGroup {...args}>
        <TabList>
            <Tab>Tab 1</Tab>
            <Tab>Tab 2</Tab>
            <Tab>Tab 3</Tab>
        </TabList>
        <TabPanels>
            <TabPanel>Tab Panel 1</TabPanel>
            <TabPanel>Tab Panel 2</TabPanel>
            <TabPanel>Tab Panel 3</TabPanel>
        </TabPanels>
    </TabGroup>
);

export const Default = Template.bind({});

export const Large = Template.bind({});
Large.args = {
    size: 'lg',
};
