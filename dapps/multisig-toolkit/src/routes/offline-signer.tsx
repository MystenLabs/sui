// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	useCurrentAccount,
	useSignTransactionBlock,
	useSuiClient,
	useSuiClientContext,
} from '@mysten/dapp-kit';
import { TransactionBlock } from '@mysten/sui.js/transactions';
import { useMutation } from '@tanstack/react-query';
import { AlertCircle, Terminal } from 'lucide-react';
import { useEffect, useState } from 'react';

import { ConnectWallet } from '@/components/connect';
import { EffectsPreview } from '@/components/preview-effects/EffectsPreview';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';

export default function OfflineSigner() {
	const currentAccount = useCurrentAccount();

	const client = useSuiClient();
	const { selectNetwork } = useSuiClientContext();

	const { mutateAsync: signTransactionBlock } = useSignTransactionBlock();
	const [tab, setTab] = useState<'transaction' | 'signature'>('transaction');
	const [bytes, setBytes] = useState('');
	const { mutate, data, isPending } = useMutation({
		mutationKey: ['sign'],
		mutationFn: async () => {
			const transactionBlock = TransactionBlock.from(bytes);
			return signTransactionBlock({ transactionBlock });
		},
		onSuccess() {
			setTab('signature');
		},
	});

	useEffect(() => {
		if (!currentAccount?.chains[0]) return;

		selectNetwork(currentAccount?.chains[0]);
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [currentAccount]);

	// runs a dry-run for the transaction based on the connected wallet.
	const {
		mutate: dryRun,
		data: dryRunData,
		isPending: dryRunLoading,
		error,
	} = useMutation({
		mutationKey: ['dry-run'],
		mutationFn: async () => {
			if (!client) throw new Error('No chain detected for the account.');

			return await client.dryRunTransactionBlock({
				transactionBlock: bytes,
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
						<Textarea value={bytes} onChange={(e) => setBytes(e.target.value.trim())} />
						<div className="flex gap-4 mb-3">
							<ConnectWallet />
							<Button disabled={!currentAccount || !bytes || isPending} onClick={() => mutate()}>
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
								<EffectsPreview output={dryRunData} />
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
