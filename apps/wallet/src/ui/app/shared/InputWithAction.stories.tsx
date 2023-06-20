// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from '@storybook/client-api';
import { type Meta, type StoryObj } from '@storybook/react';
import { Form, Formik } from 'formik';

import { InputWithAction } from './InputWithAction';

export default {
	component: InputWithAction,
	decorators: [
		(Story) => {
			const [value, setValue] = useState(1);
			return (
				<Formik
					initialValues={{ num: value }}
					onSubmit={async ({ num }) => {
						await new Promise((r) => setTimeout(r, 2000));
						setValue(num);
					}}
					enableReinitialize
				>
					<Form>
						<Story />
					</Form>
				</Formik>
			);
		},
	],
} as Meta<typeof InputWithAction>;

export const Default: StoryObj<typeof InputWithAction> = {
	render: (props) => <InputWithAction {...props} />,
	args: {
		name: 'num',
		actionText: 'Save',
		placeholder: 'Number placeholder',
	},
};
