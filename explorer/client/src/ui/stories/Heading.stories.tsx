// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentStory, type ComponentMeta } from '@storybook/react';

import { Heading } from '../Heading';

export default {
    title: 'UI/Heading',
    component: Heading,
} as ComponentMeta<typeof Heading>;

const Template: ComponentStory<typeof Heading> = (args) => (
    <div>
        <Heading {...args} weight="bold">
            This is a sample heading.
        </Heading>
        <Heading {...args} weight="semibold">
            This is a sample heading.
        </Heading>
        <Heading {...args} weight="medium">
            This is a sample heading.
        </Heading>
    </div>
);

export const Heading1 = Template.bind({});
Heading1.args = { as: 'h1', variant: 'heading1' };

export const Heading2 = Template.bind({});
Heading2.args = { as: 'h2', variant: 'heading2' };

export const Heading3 = Template.bind({});
Heading3.args = { as: 'h3', variant: 'heading3' };

export const Heading4 = Template.bind({});
Heading4.args = { as: 'h4', variant: 'heading4' };

export const Heading5 = Template.bind({});
Heading5.args = { as: 'h5', variant: 'heading5' };

export const Heading6 = Template.bind({});
Heading6.args = { as: 'h6', variant: 'heading6' };
