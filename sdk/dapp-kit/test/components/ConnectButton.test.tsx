// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import { ConnectButton } from '../../src/components/ConnectButton';
import { createWalletProviderContextWrapper } from '../test-utils';

describe('ConnectButton', () => {
	test('clicking the button opens the connect modal', async () => {
		const wrapper = createWalletProviderContextWrapper();

		render(<ConnectButton />, { wrapper });

		const connectButtonEl = screen.getByRole('button', { name: 'Connect Wallet' });
		expect(connectButtonEl).toBeInTheDocument();

		const user = userEvent.setup();
		await user.click(connectButtonEl);

		expect(screen.getByRole('dialog')).toBeInTheDocument();
	});
});
