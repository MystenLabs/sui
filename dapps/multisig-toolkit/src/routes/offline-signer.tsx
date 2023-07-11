// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ConnectWallet } from '@/components/connect';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';
import { TransactionBlock } from '@mysten/sui.js';
import { useWalletKit } from '@mysten/wallet-kit';
import { Terminal } from 'lucide-react';
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

			<Tabs value={tab} onChange={() => console.log('change')} className="w-full">
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
						</div>
					</div>
				</TabsContent>

				<TabsContent value="signature">
					<div className="border text-mono break-all rounded p-4">{data?.signature}</div>
				</TabsContent>
			</Tabs>
		</div>
	);
}
