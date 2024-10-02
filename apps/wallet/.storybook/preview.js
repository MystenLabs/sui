// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import '../src/ui/styles/global.scss';
import '@fontsource-variable/inter';
import '@fontsource-variable/red-hat-mono';
import 'bootstrap-icons/font/bootstrap-icons.scss';

import { MemoryRouter } from 'react-router-dom';

export const parameters = {
	actions: { argTypesRegex: '^on[A-Z].*' },
	controls: {
		matchers: {
			color: /(background|color)$/i,
			date: /Date$/,
		},
	},
	viewport: {
		viewports: {
			extension: {
				name: 'Chrome Extension',
				styles: {
					height: '600px',
					width: '360px',
				},
				type: 'mobile',
			},
		},
		defaultViewport: 'extension',
	},
};

export const decorators = [
	(Story) => (
		<MemoryRouter>
			<Story />
		</MemoryRouter>
	),
];
