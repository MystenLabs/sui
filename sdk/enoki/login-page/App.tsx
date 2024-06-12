// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import { useAuthCallback, useEnokiFlow, useZkLogin } from '../src/react.tsx';

export function App() {
	const flow = useEnokiFlow();
	const zkLogin = useZkLogin();
	const [result] = useState<any>(null);

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

			{result && <div>{JSON.stringify(result)}</div>}
		</div>
	);
}
