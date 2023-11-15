// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient } from '@mysten/sui.js/client';
import { parseSerializedSignature, PublicKey, SignatureScheme } from '@mysten/sui.js/cryptography';
import { AlertCircle } from 'lucide-react';
import { useState } from 'react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';

export default function BroadcastTransaction() {
	const [signature, setSignature] = useState('');
	const [transaction, setTransaction] = useState('');
	const [error, setError] = useState<Error | null>(null);
	const [digest, setDigest] = useState('');
	const client = new SuiClient({
		url: 'https://fullnode.mainnet.sui.io:443',
	});

	return (
		<div className="flex flex-col gap-4">
			<h2 className="scroll-m-20 text-4xl font-extrabold tracking-tight lg:text-5xl">
				Broadcast + Execute Transaction
			</h2>

			{error && (
				<Alert variant="destructive">
					<AlertCircle className="h-4 w-4" />
					<AlertTitle>Error</AlertTitle>
					<AlertDescription>{error.message}</AlertDescription>
				</Alert>
			)}

			<form
				className="flex flex-col gap-4"
				onSubmit={async (e) => {
					e.preventDefault();
					setError(null);

					try {
						const parsedSignature = parseSerializedSignature(signature);
						const parsedTransaction = transaction;
						const response = await client.executeTransactionBlock({
							transactionBlock: parsedTransaction,
							signature: parsedSignature.serializedSignature,
						});
						setDigest(response.digest);
					} catch (e) {
						setError(e as Error);
					}
				}}
			>
				<div className="grid w-full gap-1.5">
					<Label htmlFor="bytes">Transaction Bytes (base64 encoded)</Label>
					<Textarea
						id="bytes"
						rows={4}
						value={transaction}
						onChange={(e) => setTransaction(e.target.value)}
					/>
				</div>
				<div className="grid w-full gap-1.5">
					<Label htmlFor="bytes">Signature Bytes (base64 encoded)</Label>
					<Textarea
						id="bytes"
						rows={4}
						value={signature}
						onChange={(e) => setSignature(e.target.value)}
					/>
				</div>
				<div>
					<Button type="submit">Broadcast Transaction</Button>
				</div>
			</form>

			{digest && (
				<Card key={digest}>
					<CardHeader>
						<CardTitle>Sui Transaction Digest</CardTitle>
						<CardDescription>
							View TX Digest on{' '}
							<a className="text-blue-500" href={`https://suiexplorer.com/txblock/${digest}`}>
								Sui Explorer
							</a>
						</CardDescription>
					</CardHeader>
					<CardContent>
						<div className="flex flex-col gap-2">
							<div className="bg-muted rounded text-sm font-mono p-2 break-all">{digest}</div>
						</div>
					</CardContent>
				</Card>
			)}
		</div>
	);
}
