// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type StoryObj, type Meta } from '@storybook/react';
import { MemoryRouter } from 'react-router-dom';

import { Button, type ButtonProps } from '../Button';

export default {
    component: Button,
    decorators: [
        (Story) => (
            <MemoryRouter>
                <Story />
            </MemoryRouter>
        ),
    ],
} as Meta;

export const Primary: StoryObj<ButtonProps> = {
    render: (props) => (
        <div className="flex flex-col items-start gap-2">
            <Button to="/relative" {...props}>
                Router Link
            </Button>
            <Button to="/relative" size="lg" {...props}>
                Large Router Link
            </Button>
            <Button href="https://google.com" {...props}>
                External Link
            </Button>
            <Button href="https://google.com" size="lg" {...props}>
                Large External Link
            </Button>
            <Button onClick={() => alert('on click')} {...props}>
                Button
            </Button>
            <Button onClick={() => alert('on click')} size="lg" {...props}>
                Large Button
            </Button>
            <Button disabled {...props}>
                Disabled
            </Button>
        </div>
    ),
    args: { variant: 'primary' },
};

export const Secondary: StoryObj<ButtonProps> = {
    ...Primary,
    args: { variant: 'secondary' },
};

export const Outline: StoryObj<ButtonProps> = {
    ...Primary,
    args: { variant: 'outline' },
};
