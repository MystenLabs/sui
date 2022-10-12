// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentMeta, type Story } from '@storybook/react';
import { Fragment } from 'react';

import { Text, type TextProps } from '../Text';

export default {
    title: 'UI/Text',
    component: Text,
} as ComponentMeta<typeof Text>;

const Template: Story<{
    variants: TextProps['variant'][];
    weights?: TextProps['weight'][];
    italic?: boolean;
}> = ({ variants, weights = ['medium', 'semibold'], italic }) => (
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
);

export const Body = Template.bind({});
Body.args = {
    variants: ['body', 'bodySmall'],
    italic: true,
};

export const Subtitle = Template.bind({});
Subtitle.args = {
    variants: ['subtitle', 'subtitleSmall', 'subtitleSmallExtra'],
};

export const Caption = Template.bind({});
Caption.args = {
    variants: ['caption', 'captionSmall'],
    weights: ['medium', 'semibold', 'bold'],
};
