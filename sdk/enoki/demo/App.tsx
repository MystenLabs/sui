// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	ConnectButton,
	useConnectWallet,
	useCurrentAccount,
	useSignAndExecuteTransaction,
} from '@mysten/dapp-kit';
import { Transaction } from '@mysten/sui/transactions';
import { useState } from 'react';

import { useEnokiWallets } from '../src/react.js';
import { EnokiWallet } from '../src/wallet/index.js';

export function App() {
	const { mutate: connect } = useConnectWallet();
	const currentAccount = useCurrentAccount();
	const [result, setResult] = useState<any>();

	const { wallets, execute } = useEnokiWallets();
	const { mutateAsync: signAndExecute } = useSignAndExecuteTransaction({
		execute,
	});

	return (
		<div>
			<ConnectButton walletFilter={(wallet) => !(wallet instanceof EnokiWallet)} />
			<button
				disabled={!!currentAccount}
				onClick={() => {
					connect({ wallet: wallets.google! });
				}}
			>
				{currentAccount?.address ?? 'Login with Google'}
			</button>

			{currentAccount && (
				<button
					onClick={async () => {
						try {
							const transaction = new Transaction();
							transaction.moveCall({
								target:
									'0xfa0e78030bd16672174c2d6cc4cd5d1d1423d03c28a74909b2a148eda8bcca16::clock::access',
								arguments: [transaction.object('0x6')],
							});

							const result = await signAndExecute({ transaction });
							setResult(result.digest);
						} catch (e) {
							console.log(e);
							setResult({ error: (e as Error).stack });
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
