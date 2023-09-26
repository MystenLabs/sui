// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClientProvider } from '@mysten/dapp-kit';
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
import { ObjectModal, type ObjectModalProps } from '../Modal/ObjectModal';

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
					<SuiClientProvider>
						<Story />
					</SuiClientProvider>
				</QueryClientProvider>
			</MemoryRouter>
		),
	],
} as Meta;

export const Default: StoryObj<ModalProps> = {
	args: {},
};

export const Image: StoryObj<ObjectModalProps> = {
	name: 'Image Modal',
	render: () => {
		const [open, setOpen] = useState(true);
		return (
			<div>
				<ObjectModal
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

export const Video: StoryObj<ObjectModalProps> = {
	name: 'Video Modal',
	render: () => {
		const [open, setOpen] = useState(true);
		return (
			<div>
				<ObjectModal
					title="Sui"
					open={open}
					src="https://images.unsplash.com/photo-1562016600-ece13e8ba570?auto=format&fit=crop&w=738&q=80"
					video="https://interactive-examples.mdn.mozilla.net/media/cc0-videos/flower.webm"
					alt="Sui"
					onClose={() => setOpen(false)}
					subtitle="Still water runs deep."
				/>

				<Link onClick={() => setOpen(true)}>Click to open</Link>
			</div>
		);
	},
};
