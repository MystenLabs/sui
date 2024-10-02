// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import { Transaction } from '@mysten/sui/transactions';
import { useState } from 'react';

import { useAuthCallback, useEnokiFlow, useZkLogin } from '../src/react.tsx';

export function App() {
	const flow = useEnokiFlow();
	const zkLogin = useZkLogin();
	const [result, setResult] = useState<any>(null);

	useAuthCallback();

	return (
		<div>
			<div>Address: {zkLogin.address}</div>
			<div>Provider: {zkLogin.provider}</div>
			{!zkLogin.address ? (
				<button
					onClick={async () => {
						window.location.href = await flow.createAuthorizationURL({
							provider: 'google',
							clientId: '705781974144-cltddr1ggjnuc3kaimtc881r2n5bderc.apps.googleusercontent.com',
							redirectUrl: window.location.href.split('#')[0],
						});
					}}
				>
					Sign in with Google
				</button>
			) : (
				<button onClick={() => flow.logout()}>Sign Out</button>
			)}

			{zkLogin.address && (
				<button
					onClick={async () => {
						try {
							const transaction = new Transaction();
							transaction.moveCall({
								target:
									'0xfa0e78030bd16672174c2d6cc4cd5d1d1423d03c28a74909b2a148eda8bcca16::clock::access',
								arguments: [transaction.object('0x6')],
							});

							const result = await flow.sponsorAndExecuteTransaction({
								network: 'testnet',
								// @ts-expect-error: Type references not quite doing their thing:
								client: new SuiClient({ url: getFullnodeUrl('testnet') }),
								// @ts-expect-error: Type references not quite doing their thing:
								transaction,
							});

							setResult(result);
						} catch (e) {
							console.log(e);
							setResult({ error: e });
						}
					}}
				>
					Sign transaction
				</button>
			)}

			{result && <div>{JSON.stringify(result)}</div>}
		</div>
	);
}
