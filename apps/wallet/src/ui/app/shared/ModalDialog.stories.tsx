// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { useState } from 'react';

import { Button } from './ButtonUI';
import { ModalDialog } from './ModalDialog';

export default {
	component: ModalDialog,
	decorators: [
		(Story, ctx) => {
			const [isOpen, setIsOpen] = useState(false);
			return (
				<>
					<Button onClick={() => setIsOpen(true)} text="Show dialog" />
					<Story
						args={{
							...ctx.args,
							isOpen,
							onClose: () => setIsOpen(false),
						}}
					/>
				</>
			);
		},
	],
} as Meta<typeof ModalDialog>;

export const Default: StoryObj<typeof ModalDialog> = {
	render: (props) => (
		<>
			<ModalDialog {...props} />
		</>
	),
	args: {
		title: 'Test Modal Dialog',
		body: 'Hello this is a modal',
		footer: 'Test footer content',
	},
};
