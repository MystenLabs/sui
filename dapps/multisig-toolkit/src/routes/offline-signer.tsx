// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ConnectWallet } from '@/components/connect';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';
import {
	Connection,
	JsonRpcProvider,
	TransactionBlock,
	devnetConnection,
	mainnetConnection,
	testnetConnection,
} from '@mysten/sui.js';
import { useWalletKit } from '@mysten/wallet-kit';
import { AlertCircle, Terminal } from 'lucide-react';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';

export default function OfflineSigner() {
	const { currentAccount, signTransactionBlock } = useWalletKit();
	const [tab, setTab] = useState<'transaction' | 'signature'>('transaction');
	const [bytes, setBytes] = useState('');
	const { mutate, data, isLoading } = useMutation({
		mutationKey: ['sign'],
		mutationFn: async () => {
			const transactionBlock = TransactionBlock.from(bytes);
			return signTransactionBlock({ transactionBlock });
		},
		onSuccess() {
			setTab('signature');
		},
	});

	// supported connections.
	const connections: Record<`${string}:${string}`, Connection> = {
		'sui:testnet': testnetConnection,
		'sui:mainnet': mainnetConnection,
		'sui:devnet': devnetConnection,
	};

	// runs a dry-run for the transaction based on the connected wallet.
	const {
		mutate: dryRun,
		data: dryRunData,
		isLoading: dryRunLoading,
		error,
		reset,
	} = useMutation({
		mutationKey: ['dry-run'],
		mutationFn: async () => {
			if (!currentAccount?.chains[0]) throw new Error('No chain detected for the account.');
			const provider = new JsonRpcProvider(
				connections[currentAccount?.chains.filter((x) => x.startsWith('sui'))[0]],
			);

			const transactionBlock = TransactionBlock.from(bytes);

			return await provider.devInspectTransactionBlock({
				transactionBlock,
				sender: currentAccount?.address,
			});
		},
	});

	return (
		<div className="flex flex-col gap-4">
			<h2 className="scroll-m-20 text-4xl font-extrabold tracking-tight lg:text-5xl">
				Offline Signer
			</h2>

			{!currentAccount && (
				<Alert>
					<Terminal className="h-4 w-4" />
					<AlertTitle>Wallet Required</AlertTitle>
					<AlertDescription>
						Signing a transaction requires you to first connect to a wallet.
					</AlertDescription>
				</Alert>
			)}

			<Tabs value={tab} className="w-full">
				<TabsList className="w-full">
					<TabsTrigger className="flex-1" value="transaction" onClick={() => setTab('transaction')}>
						Transaction
					</TabsTrigger>
					<TabsTrigger
						className="flex-1"
						value="signature"
						disabled={!data}
						onClick={() => setTab('signature')}
					>
						Signature
					</TabsTrigger>
				</TabsList>

				<TabsContent value="transaction">
					<div className="flex flex-col items-start gap-4">
						<Textarea value={bytes} onChange={(e) => setBytes(e.target.value)} />
						<div className="flex gap-4">
							<ConnectWallet />
							<Button disabled={!currentAccount || !bytes || isLoading} onClick={() => mutate()}>
								Sign Transaction
							</Button>
							<Button
								variant="outline"
								disabled={!currentAccount || !bytes || dryRunLoading}
								onClick={() => dryRun()}
							>
								Preview Effects
							</Button>
						</div>
						{dryRunData && (
							<>
								<Button variant="link" size="sm" onClick={() => reset()}>
									Hide
								</Button>
								<Textarea value={JSON.stringify(dryRunData, null, 4)} rows={20} />
							</>
						)}
					</div>
				</TabsContent>

				<TabsContent value="signature">
					<div className="border text-mono break-all rounded p-4">{data?.signature}</div>
				</TabsContent>
			</Tabs>
			{(error as Error) && (
				<Alert variant="default">
					<AlertCircle className="h-4 w-4" />
					<AlertTitle>Error</AlertTitle>
					<AlertDescription>{(error as Error).message}</AlertDescription>
				</Alert>
			)}
		</div>
	);
}
