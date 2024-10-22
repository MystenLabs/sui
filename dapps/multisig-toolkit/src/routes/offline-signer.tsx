// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCurrentAccount, useSignTransaction, useSuiClientContext } from '@mysten/dapp-kit';
import { getFullnodeUrl, SuiClient } from '@mysten/sui/client';
import { messageWithIntent } from '@mysten/sui/cryptography';
import { Transaction } from '@mysten/sui/transactions';
import { fromBase64, toHex } from '@mysten/sui/utils';
import { blake2b } from '@noble/hashes/blake2b';
import { useMutation } from '@tanstack/react-query';
import { AlertCircle, Terminal } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';

import { ConnectWallet } from '@/components/connect';
import { DryRunProvider, type Network } from '@/components/preview-effects/DryRunContext';
import { EffectsPreview } from '@/components/preview-effects/EffectsPreview';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Textarea } from '@/components/ui/textarea';

export default function OfflineSigner() {
	const currentAccount = useCurrentAccount();

	const [dryRunNetwork, setDryRunNetwork] = useState<Network>('mainnet');

	const { selectNetwork } = useSuiClientContext();

	const { mutateAsync: signTransaction } = useSignTransaction();
	const [tab, setTab] = useState<'transaction' | 'signature'>('transaction');
	const [bytes, setBytes] = useState('');
	const { mutate, data, isPending } = useMutation({
		mutationKey: ['sign'],
		mutationFn: async () => {
			const transaction = Transaction.from(bytes);
			return signTransaction({ transaction });
		},
		onSuccess() {
			setTab('signature');
		},
	});

	useEffect(() => {
		if (!currentAccount?.chains[0]) return;
		selectNetwork(currentAccount.chains[0]);
		const activeNetwork = (
			currentAccount.chains[0].includes(':')
				? currentAccount.chains[0].split(':')[1]
				: currentAccount.chains[0]
		) as Network;
		setDryRunNetwork(activeNetwork);
		// eslint-disable-next-line react-hooks/exhaustive-deps
	}, [currentAccount]);

	// runs a dry-run for the transaction based on the connected wallet.
	const {
		mutate: dryRun,
		data: dryRunData,
		isPending: dryRunLoading,
		error,
	} = useMutation({
		mutationKey: [dryRunNetwork, 'dry-run'],
		mutationFn: async () => {
			const dryRunClient = new SuiClient({
				url: getFullnodeUrl(dryRunNetwork),
			});
			const transaction = Transaction.from(bytes);
			return await dryRunClient.dryRunTransactionBlock({
				transactionBlock: await transaction.build({
					client: dryRunClient,
				}),
			});
		},
	});

	// Step 3: compute the blake2b hash
	const ledgerTransactionHash = useMemo(() => {
		if (!bytes) return null;
		try {
			// Decode the base64-encoded transaction bytes
			const decodedBytes = fromBase64(bytes);
			const intentMessage = messageWithIntent('TransactionData', decodedBytes);
			const intentMessageDigest = blake2b(intentMessage, { dkLen: 32 });
			const intentMessageDigestHex = toHex(intentMessageDigest);
			return `0x${intentMessageDigestHex}`;
		} catch (error) {
			return 'Error computing hash';
		}
	}, [bytes]);

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
					<div className="grid grid-cols-1 gap-4">
						<Textarea value={bytes} onChange={(e) => setBytes(e.target.value.trim())} />
						<div className="grid md:grid-cols-2 gap-5">
							<div className="flex gap-5">
								<ConnectWallet />
								<Button disabled={!currentAccount || !bytes || isPending} onClick={() => mutate()}>
									Sign Transaction
								</Button>
							</div>

							<div className="justify-between md:justify-end flex gap-5">
								<Button
									variant="outline"
									className="flex-shrink-0 max-md:w-1/2 h-full"
									disabled={!dryRunNetwork || !bytes || dryRunLoading}
									onClick={() => dryRun()}
								>
									Preview Effects
								</Button>
								<div className="grid max-md:w-full gap-1.5">
									<select
										id="dry-run-network"
										className="bg-background border px-6 rounded-sm p-3 text-white"
										value={dryRunNetwork}
										onChange={(e) =>
											setDryRunNetwork(
												e.target.value as 'mainnet' | 'testnet' | 'devnet' | 'localnet',
											)
										}
									>
										<option value="mainnet">Mainnet</option>
										<option value="testnet">Testnet</option>
										<option value="devnet">Devnet</option>
										<option value="localnet">Localnet</option>
									</select>
								</div>
							</div>
						</div>
						{dryRunData && (
							<DryRunProvider network={dryRunNetwork}>
								<EffectsPreview output={dryRunData} network={dryRunNetwork} />
							</DryRunProvider>
						)}

						{ledgerTransactionHash && (
							<div>
								<h4 className="text-lg font-semibold">Ledger Transaction Hash</h4>
								<div className="border text-mono break-all rounded p-4">
									{ledgerTransactionHash}
								</div>
							</div>
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
