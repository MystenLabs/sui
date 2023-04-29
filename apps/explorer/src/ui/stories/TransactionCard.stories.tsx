// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';

import {
    TransactionCard,
    type TransactionCardProps,
    TransactionCardSection,
} from '~/ui/TransactionCard';

export default {
    component: TransactionCard,
} as Meta;

export const Default: StoryObj<TransactionCardProps> = {
    render: (props) => {
        const sections = Array(5)
            .fill(true)
            .map((_, index) => <div key={index}>Section Item {index}</div>);

        return (
            <div className="h-[1000px]">
                <TransactionCard collapsible title="Card Title" {...props}>
                    {sections.map((section, index) => (
                        <TransactionCardSection
                            key={index}
                            title={`Section Title ${index}`}
                        >
                            {section}
                        </TransactionCardSection>
                    ))}
                </TransactionCard>
            </div>
        );
    },
};

export const Small: StoryObj<TransactionCardProps> = {
    ...Default,
    args: { size: 'sm' },
};
