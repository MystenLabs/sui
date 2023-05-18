// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { RpcClientContext } from '@mysten/core';
import { type Meta, type StoryObj } from '@storybook/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
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
import { ImageModal, type ImageModalProps } from '../Modal/ImageModal';

import { DefaultRpcClient, Network } from '~/utils/api/DefaultRpcClient';

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
                <QueryClientProvider client={new QueryClient()}>
                    <RpcClientContext.Provider
                        value={DefaultRpcClient(Network.LOCAL)}
                    >
                        <Story />
                    </RpcClientContext.Provider>
                </QueryClientProvider>
            </MemoryRouter>
        ),
    ],
} as Meta;

export const Default: StoryObj<ModalProps> = {
    args: {},
};

export const Image: StoryObj<ImageModalProps> = {
    name: 'Image Modal',
    render: () => {
        const [open, setOpen] = useState(true);
        return (
            <div>
                <ImageModal
                    title="Sui"
                    open={open}
                    src="https://images.unsplash.com/photo-1562016600-ece13e8ba570?auto=format&fit=crop&w=738&q=80"
                    alt="Sui"
                    onClose={() => setOpen(false)}
                    subtitle="Still water runs deep."
                />

                <Link onClick={() => setOpen(true)}>Click to open</Link>
            </div>
        );
    },
};
