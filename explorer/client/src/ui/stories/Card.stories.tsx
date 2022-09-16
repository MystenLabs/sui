// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentStory, type ComponentMeta } from '@storybook/react';

import { Card } from '../Card';

export default {
    title: 'UI/Card',
    component: Card,
} as ComponentMeta<typeof Card>;

const Template: ComponentStory<typeof Card> = (args) => (
    <Card {...args}>This is sample content within the card.</Card>
);

export const Default = Template.bind({});
