// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type Meta, type StoryObj } from '@storybook/react';
import { useState } from 'react';

import { Button } from './ButtonUI';
import { ConfirmationModal } from './ConfirmationModal';

export default {
	component: ConfirmationModal,
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
							onResponse: () => setIsOpen(false),
						}}
					/>
				</>
			);
		},
	],
} as Meta<typeof ConfirmationModal>;

export const Default: StoryObj<typeof ConfirmationModal> = {
	render: (props) => (
		<>
			<ConfirmationModal {...props} />
		</>
	),
};
