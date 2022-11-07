// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { Fragment } from 'react';

import { Text, type TextProps } from '../Text';

export default {
    component: Text,
} as Meta;

interface StoryProps {
    variants: TextProps['variant'][];
    weights?: TextProps['weight'][];
    italic?: boolean;
}

export const Body: StoryObj<StoryProps> = {
    render: ({ variants, weights = ['medium', 'semibold'], italic }) => (
        <div>
            {variants.map((variant) => (
                <Fragment key={variant}>
                    {weights.map((weight) => (
                        <Text key={weight} variant={variant} weight={weight}>
                            {variant} - {weight}
                        </Text>
                    ))}

                    {italic && (
                        <Text variant={variant} italic>
                            {variant} - Italic
                        </Text>
                    )}
                </Fragment>
            ))}
        </div>
    ),
    args: {
        variants: ['body', 'bodySmall'],
        italic: true,
    },
};

export const Subtitle: StoryObj<StoryProps> = {
    ...Body,
    args: {
        variants: ['subtitle', 'subtitleSmall', 'subtitleSmallExtra'],
    },
};

export const Caption: StoryObj<StoryProps> = {
    ...Body,
    args: {
        variants: ['caption', 'captionSmall'],
        weights: ['medium', 'semibold', 'bold'],
    },
};
