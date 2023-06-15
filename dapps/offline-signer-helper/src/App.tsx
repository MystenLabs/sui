// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SignedTransaction, TransactionBlock } from '@mysten/sui.js';
import { ConnectButton, useWalletKit } from '@mysten/wallet-kit';
import { useState } from 'react';

export function App() {
	const { currentAccount, signTransactionBlock } = useWalletKit();
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<Error | null>(null);
	const [signedTx, setSignedTx] = useState<SignedTransaction | null>(null);

	return (
		<div className="m-8 px-8 py-6 mx-auto bg-white rounded-2xl shadow-md max-w-lg w-full">
			<div className="flex items-center justify-between mb-4">
				<h1 className="font-bold text-2xl">Offline Signer Helper</h1>
				<ConnectButton
					className={currentAccount ? '!bg-white !text-gray-900' : '!bg-indigo-600 !text-white'}
				/>
			</div>

			{error && (
				<div className="bg-red-100 text-red-800 border border-red-300 px-2 py-1.5 rounded-md text-sm my-4">
					{error.message}
				</div>
			)}

			{currentAccount ? (
				<div>
					<form
						onSubmit={async (e) => {
							e.preventDefault();
							setError(null);
							setSignedTx(null);
							setLoading(true);
							try {
								const formData = new FormData(e.currentTarget);
								const signed = await signTransactionBlock({
									transactionBlock: TransactionBlock.from(formData.get('bytes') as string),
								});
								setSignedTx(signed);
							} catch (e) {
								setError(e as Error);
							} finally {
								setLoading(false);
							}
						}}
					>
						<label htmlFor="bytes" className="block text-sm font-medium leading-6 text-gray-900">
							Transaction Block Bytes (base64 encoded)
						</label>
						<div className="mt-2">
							<textarea
								id="bytes"
								name="bytes"
								rows={3}
								className="block w-full rounded-md border-0 text-gray-900 shadow-sm ring-1 ring-inset ring-gray-300 placeholder:text-gray-400 focus:ring-2 focus:ring-inset focus:ring-indigo-600 sm:py-1.5 sm:text-sm sm:leading-6"
								defaultValue=""
							/>
						</div>
						<div className="mt-2">
							<button
								type="submit"
								className="bg-indigo-600 text-sm font-medium text-white rounded-lg px-4 py-3 disabled:cursor-not-allowed disabled:opacity-60"
								disabled={loading}
							>
								Sign Transaction
							</button>
						</div>
					</form>

					{signedTx && (
						<div className="mt-4">
							<div>
								<div className="text-lg font-bold mb-2">Signed Transaction</div>
								<code className="block bg-gray-200 p-2 text-gray-800 rounded text-sm break-all whitespace-pre-wrap">
									{signedTx.signature}
								</code>
							</div>
						</div>
					)}
				</div>
			) : (
				<div className="p-12 text-center font-gray-600">Connect your wallet to get started.</div>
			)}
		</div>
	);
}
