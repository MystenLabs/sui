// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import { parseSerializedSignature } from '@mysten/sui/cryptography';
import { useMutation } from '@tanstack/react-query';
import { AlertCircle } from 'lucide-react';
import { useState } from 'react';

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';

type NetworkType = 'mainnet' | 'testnet' | 'devnet';

export default function ExecuteTransaction() {
	const [network, setNetwork] = useState<NetworkType>('mainnet');
	const [tab, setTab] = useState<'transaction' | 'signature' | 'digest'>('transaction');
	const [transaction, setTransaction] = useState('');
	const [signature, setSignature] = useState('');

	const rpcUrl = getFullnodeUrl(network);
	const client = new SuiClient({
		url: rpcUrl,
	});
	//const client = useSuiClient();

	const {
		mutate,
		data: digest,
		error,
		isPending,
	} = useMutation({
		mutationKey: ['broadcast'],
		mutationFn: async () => {
			const parsedSignature = parseSerializedSignature(signature);
			const response = await client.executeTransactionBlock({
				transactionBlock: transaction,
				signature: parsedSignature.serializedSignature,
			});
			return response.digest;
		},
		onSuccess() {
			setTab('digest');
		},
	});

	const handleSubmit = (e) => {
		e.preventDefault();
		const signature = e.target.signature.value;
		const transaction = e.target.transaction.value;
		setTransaction(transaction);
		setSignature(signature);
		mutate();
	};

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
			<Tabs value={tab} className="w-full">
				<TabsList className="w-full">
					<TabsTrigger className="flex-1" value="transaction" onClick={() => setTab('transaction')}>
						Transaction
					</TabsTrigger>
					<TabsTrigger
						className="flex-1"
						value="digest"
						disabled={!digest}
						onClick={() => setTab('digest')}
					>
						Digest
					</TabsTrigger>
				</TabsList>
				<TabsContent value="transaction">
					<form className="flex flex-col gap-4" onSubmit={handleSubmit}>
						<div className="grid w-full gap-1.5">
							<Label htmlFor="network">Select Network</Label>
							<select
								id="network"
								className="bg-background border rounded-sm p-3 text-white"
								value={network}
								onChange={(e) => setNetwork(e.target.value as NetworkType)}
							>
								<option value="devnet">Devnet</option>
								<option value="testnet">Testnet</option>
								<option value="mainnet">Mainnet</option>
							</select>
						</div>
						<div className="grid w-full gap-1.5">
							<Label htmlFor="transaction">Transaction Bytes (base64 encoded)</Label>
							<Textarea id="transaction" name="transaction" rows={4} />
						</div>
						<div className="grid w-full gap-1.5">
							<Label htmlFor="signature">Signature Bytes (base64 encoded)</Label>
							<Textarea id="signature" name="signature" rows={4} />
						</div>
						<div>
							<Button type="submit" disabled={isPending}>
								Broadcast Transaction
							</Button>
						</div>
					</form>
				</TabsContent>

				<TabsContent value="digest">
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
				</TabsContent>
			</Tabs>
		</div>
	);
}
