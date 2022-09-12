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

export const H1 = Template.bind({});
H1.args = { tag: 'h1', variant: 'h1' };

export const H2 = Template.bind({});
H2.args = { tag: 'h2', variant: 'h2' };

export const H3 = Template.bind({});
H3.args = { tag: 'h3', variant: 'h3' };

export const H4 = Template.bind({});
H4.args = { tag: 'h4', variant: 'h4' };

export const H5 = Template.bind({});
H5.args = { tag: 'h5', variant: 'h5' };

export const H6 = Template.bind({});
H6.args = { tag: 'h6', variant: 'h6' };
