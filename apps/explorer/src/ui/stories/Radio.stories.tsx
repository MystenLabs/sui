// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type StoryObj, type Meta } from '@storybook/react';
import { useState } from 'react';

import { RadioGroup, type RadioGroupProps, RadioOption } from '~/ui/Radio';

export default {
    component: RadioGroup,
} as Meta;

const groups = [
    {
        label: 'label 1',
        description: 'description 1',
    },
    {
        label: 'label 2',
        description: 'description 2',
    },
    {
        label: 'label 3',
        description: 'description 3',
    },
];

export const Default: StoryObj<RadioGroupProps> = {
    render: (props) => {
        const [selected, setSelected] = useState(groups[0]);

        return (
            <div>
                <RadioGroup
                    {...props}
                    className="flex"
                    value={selected}
                    onChange={setSelected}
                    ariaLabel="Default radio group"
                >
                    {groups.map((group) => (
                        <RadioOption
                            key={group.label}
                            value={group}
                            label={group.label}
                        />
                    ))}
                </RadioGroup>
            </div>
        );
    },
};
