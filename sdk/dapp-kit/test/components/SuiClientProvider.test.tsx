// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { render } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { screen } from '@testing-library/dom';
import { SuiClientProvider } from '../../src/components/SuiClientProvider.js';
import { useSuiClient, useSuiClientContext } from 'dapp-kit/src/index.js';
import { SuiClient } from '@mysten/sui.js/client';
import { useState } from 'react';

describe('SuiClientProvider', () => {
	it('renders without crashing', () => {
		render(
			<SuiClientProvider>
				<div>Test</div>
			</SuiClientProvider>,
		);
		expect(screen.getByText('Test')).toBeInTheDocument();
	});

	it('provides a SuiClient instance to its children', () => {
		const ChildComponent = () => {
			const client = useSuiClient();
			expect(client).toBeInstanceOf(SuiClient);
			return <div>Test</div>;
		};

		render(
			<SuiClientProvider>
				<ChildComponent />
			</SuiClientProvider>,
		);
	});

	it('can accept pre-configured SuiClients', () => {
		const suiClient = new SuiClient({ url: 'http://localhost:8080' });
		const ChildComponent = () => {
			const client = useSuiClient();
			expect(client).toBeInstanceOf(SuiClient);
			expect(client).toBe(suiClient);
			return <div>Test</div>;
		};

		render(
			<SuiClientProvider networks={{ localnet: suiClient }}>
				<ChildComponent />
			</SuiClientProvider>,
		);

		expect(screen.getByText('Test')).toBeInTheDocument();
	});

	test('can create sui clients with custom options', async () => {
		function NetworkSelector() {
			const ctx = useSuiClientContext();

			return (
				<div>
					{Object.keys(ctx.networks).map((network) => (
						<button key={network} onClick={() => ctx.selectNetwork(network)}>
							{`select ${network}`}
						</button>
					))}
				</div>
			);
		}
		function CustomConfigProvider() {
			const [selectedNetwork, setSelectedNetwork] = useState<string>();

			return (
				<SuiClientProvider
					networks={{
						a: {
							url: 'http://localhost:8080',
							custom: setSelectedNetwork,
						},
						b: {
							url: 'http://localhost:8080',
							custom: setSelectedNetwork,
						},
					}}
					createClient={(name, { custom, ...config }) => {
						custom(name);
						return new SuiClient(config);
					}}
				>
					<div>{`selected network: ${selectedNetwork}`}</div>
					<NetworkSelector />
				</SuiClientProvider>
			);
		}

		const user = userEvent.setup();

		render(<CustomConfigProvider />);

		expect(screen.getByText('selected network: a')).toBeInTheDocument();

		await user.click(screen.getByText('select b'));

		expect(screen.getByText('selected network: b')).toBeInTheDocument();
	});
});
