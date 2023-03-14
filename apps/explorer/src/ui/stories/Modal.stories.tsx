// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { useState } from 'react';
import { MemoryRouter } from 'react-router-dom';

import { Link } from '../Link';
import {
    CloseButton,
    Modal,
    ModalBody,
    ModalContent,
    ModalHeading,
    type ModalProps,
} from '../Modal';

export default {
    component: () => {
        const [open, setOpen] = useState(false);
        const onClose = () => setOpen(false);

        return (
            <div>
                <Modal open={open} onClose={onClose}>
                    <ModalContent>
                        <CloseButton onClick={onClose} />
                        <ModalHeading>Modal</ModalHeading>
                        <ModalBody>This is a modal.</ModalBody>
                    </ModalContent>
                </Modal>
                <Link onClick={() => setOpen(true)}>Show More</Link>
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

export const Default: StoryObj<ModalProps> = {
    args: {},
};
