// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import { Heading, type HeadingProps } from '../Heading';

export default {
    component: Heading,
} as Meta;

export const Heading1: StoryObj<HeadingProps> = {
    render: (props) => {
        return (
            <div className="space-y-2">
                <div>
                    <Heading {...props} weight="bold">
                        This is a sample heading.
                    </Heading>
                    <Heading {...props} weight="semibold">
                        This is a sample heading.
                    </Heading>
                    <Heading {...props} weight="medium">
                        This is a sample heading.
                    </Heading>
                </div>
                <div>
                    <Heading {...props} weight="bold" fixed>
                        This is a sample heading. (fixed)
                    </Heading>
                    <Heading {...props} weight="semibold" fixed>
                        This is a sample heading. (fixed)
                    </Heading>
                    <Heading {...props} weight="medium" fixed>
                        This is a sample heading. (fixed)
                    </Heading>
                </div>
            </div>
        );
    },
    args: { as: 'h1', variant: 'heading1' },
};

export const Heading2: StoryObj<HeadingProps> = {
    ...Heading1,
    args: { as: 'h2', variant: 'heading2' },
};

export const Heading3: StoryObj<HeadingProps> = {
    ...Heading1,
    args: { as: 'h3', variant: 'heading3' },
};

export const Heading4: StoryObj<HeadingProps> = {
    ...Heading1,
    args: { as: 'h4', variant: 'heading4' },
};

export const Heading5: StoryObj<HeadingProps> = {
    ...Heading1,
    args: { as: 'h5', variant: 'heading5' },
};

export const Heading6: StoryObj<HeadingProps> = {
    ...Heading1,
    args: { as: 'h6', variant: 'heading6' },
};
