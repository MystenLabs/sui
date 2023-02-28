// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { useState } from 'react';
import { MemoryRouter } from 'react-router-dom';

import { LightBox, type LightBoxProps } from '../LightBox';
import { Link } from '../Link';

export default {
    component: () => {
        const [open, setOpen] = useState(true);

        return (
            <div>
                <LightBox open={open} onClose={() => setOpen(false)}>
                    <img
                        alt="A Lovely Apepé"
                        src="https://ipfs.io/ipfs/bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty"
                    />
                </LightBox>
                <Link onClick={() => setOpen(true)}>View Image</Link>
            </div>
        );
    },
    decorators: [
        (Story) => (
            <MemoryRouter>
                <Story />
            </MemoryRouter>
        ),
    ],
} as Meta;

export const Default: StoryObj<LightBoxProps> = {
    args: {},
};
