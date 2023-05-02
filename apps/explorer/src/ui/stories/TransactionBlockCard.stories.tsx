// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import {
    TransactionBlockCard,
    type TransactionBlockCardProps,
    TransactionBlockCardSection,
} from '~/ui/TransactionBlockCard';

export default {
    component: TransactionBlockCard,
} as Meta;

export const Default: StoryObj<TransactionBlockCardProps> = {
    render: (props) => {
        const sections = Array(5)
            .fill(true)
            .map((_, index) => <div key={index}>Section Item {index}</div>);

        return (
            <div className="h-[1000px]">
                <TransactionBlockCard collapsible title="Card Title" {...props}>
                    {sections.map((section, index) => (
                        <TransactionBlockCardSection
                            key={index}
                            title={`Section Title ${index}`}
                        >
                            {section}
                        </TransactionBlockCardSection>
                    ))}
                </TransactionBlockCard>
            </div>
        );
    },
};

export const Small: StoryObj<TransactionBlockCardProps> = {
    ...Default,
    args: { size: 'sm' },
};
