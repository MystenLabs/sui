// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentStory, type ComponentMeta } from '@storybook/react';
import { MemoryRouter } from 'react-router-dom';

import { Button } from '../Button';

export default {
    title: 'UI/Button',
    component: Button,
    decorators: [
        (Story) => (
            <MemoryRouter>
                <Story />
            </MemoryRouter>
        ),
    ],
} as ComponentMeta<typeof Button>;

const Template: ComponentStory<typeof Button> = (args) => (
    <div className="flex flex-col items-start gap-2">
        <Button to="/relative" {...args}>
            Router Link
        </Button>
        <Button to="/relative" size="lg" {...args}>
            Large Router Link
        </Button>
        <Button href="https://google.com" {...args}>
            External Link
        </Button>
        <Button href="https://google.com" size="lg" {...args}>
            Large External Link
        </Button>
        <Button onClick={() => alert('on click')} {...args}>
            Button
        </Button>
        <Button onClick={() => alert('on click')} size="lg" {...args}>
            Large Button
        </Button>
    </div>
);

export const Primary = Template.bind({});
Primary.args = { variant: 'primary' };

export const Secondary = Template.bind({});
Secondary.args = { variant: 'secondary' };

export const Outline = Template.bind({});
Outline.args = { variant: 'outline' };
