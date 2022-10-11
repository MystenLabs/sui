// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentStory, type ComponentMeta } from '@storybook/react';

import { ListItem, VerticalList } from '../VerticalList';

export default {
    title: 'UI/VerticalList',
    component: VerticalList,
} as ComponentMeta<typeof VerticalList>;

const Template: ComponentStory<typeof VerticalList> = (args) => (
    <VerticalList {...args}>
        <ListItem>One</ListItem>
        <ListItem active>Two</ListItem>
        <ListItem>Three</ListItem>
        <ListItem>Four</ListItem>
    </VerticalList>
);

export const Default = Template.bind({});
Default.args = {};
