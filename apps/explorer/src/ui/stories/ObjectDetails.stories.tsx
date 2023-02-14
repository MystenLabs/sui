// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { MemoryRouter } from 'react-router-dom';

import { ObjectDetails, type ObjectDetailsProps } from '../ObjectDetails';

export default {
    component: ObjectDetails,
    decorators: [
        (Story) => (
            <MemoryRouter>
                <Story />
            </MemoryRouter>
        ),
    ],
} as Meta;

export const Default: StoryObj<ObjectDetailsProps> = {
    render: () => (
        <ObjectDetails
            name="Rare Apepé 4042"
            type="JPEG Image"
            variant="small"
            id="0x4897c931565428a2a3842afb523ca5559d4b6726"
            image="https://ipfs.io/ipfs/bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty"
        />
    ),
};

export const AutoModNSFW: StoryObj<ObjectDetailsProps> = {
    name: 'Auto-Mod / NSFW',
    render: () => (
        <ObjectDetails
            name="Naughty Apepé 4042"
            type="JPEG Image"
            variant="small"
            nsfw
            id="0x4897c931565428a2a3842afb523ca5559d4b6726"
            image="https://ipfs.io/ipfs/bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty"
        />
    ),
};
