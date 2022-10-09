// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    DateFilter,
    useDateFilterState,
    type DateFilterProps,
} from '../DateFilter';

import type { ComponentMeta, ComponentStory } from '@storybook/react';

export default {
    title: 'UI/DateFilter',
    component: DateFilter,
} as ComponentMeta<typeof DateFilter>;

function DateFilterWithState(
    props: Omit<DateFilterProps, 'value' | 'onChange'>
) {
    const [value, onChange] = useDateFilterState('D');
    return <DateFilter {...props} value={value} onChange={onChange} />;
}

const Template: ComponentStory<typeof DateFilter> = (args) => (
    <DateFilterWithState {...args} />
);

export const Default = Template.bind({});

export const CustomOptions = Template.bind({});
CustomOptions.args = {
    options: ['D', 'ALL'],
};
