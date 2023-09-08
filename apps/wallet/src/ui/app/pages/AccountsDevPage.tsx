// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Popover } from '@headlessui/react';
import { useFormatCoin } from '@mysten/core';
import { useBalance } from '@mysten/dapp-kit';
import { TransactionBlock } from '@mysten/sui.js/builder';
import { type ExportedKeypair } from '@mysten/sui.js/cryptography';
import { SUI_TYPE_ARG } from '@mysten/sui.js/framework';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { toB64 } from '@mysten/sui.js/utils';
import { hexToBytes } from '@noble/hashes/utils';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { Toaster, toast } from 'react-hot-toast';
import { useNavigate } from 'react-router-dom';
import { type BackgroundClient } from '../background-client';
import { ConnectLedgerModal } from '../components/ledger/ConnectLedgerModal';
import LoadingIndicator from '../components/loading/LoadingIndicator';
import Logo from '../components/logo';
import NetworkSelector from '../components/network-selector';
import { useAppSelector } from '../hooks';
import { useAccountSources } from '../hooks/useAccountSources';
import { useAccounts } from '../hooks/useAccounts';
import { useBackgroundClient } from '../hooks/useBackgroundClient';
import { useQredoTransaction } from '../hooks/useQredoTransaction';
import { useSigner } from '../hooks/useSigner';
import { Button } from '../shared/ButtonUI';
import { Card } from '../shared/card';
import { FAUCET_HOSTS } from '../shared/faucet/FaucetRequestButton';
import { useFaucetMutation } from '../shared/faucet/useFaucetMutation';
import { Heading } from '../shared/heading';
import { Text } from '../shared/text';
import { type AccountSourceSerializedUI } from '_src/background/account-sources/AccountSource';
import { type AccountType, type SerializedUIAccount } from '_src/background/accounts/Account';
import { isLedgerAccountSerializedUI } from '_src/background/accounts/LedgerAccount';
import { isMnemonicSerializedUiAccount } from '_src/background/accounts/MnemonicAccount';
import { isQredoAccountSerializedUI } from '_src/background/accounts/QredoAccount';
import { type ZkProvider } from '_src/background/accounts/zk/providers';
import { entropyToSerialized, mnemonicToEntropy } from '_src/shared/utils/bip39';

export const testPassNewAccounts = 'test';
const testMnemonic =
	'lawsuit welcome deputy faith shadow monitor common paper candy horse panda history';
const mnemonicFirstKeyPair: ExportedKeypair = {
	schema: 'ED25519',
	privateKey: toB64(hexToBytes('5051bc918ec4991c62969d6cd0f1edaabfbe5244e509d7a96f39fe52e76cf54f')),
};
const typeOrder: Record<AccountType, number> = {
	zk: 0,
	'mnemonic-derived': 1,
	imported: 2,
	ledger: 3,
	qredo: 4,
};

/**
 * Just for dev, to allow testing new accounts handling
 */
export function AccountsDev() {
	const accountSources = useAccountSources();
	const accounts = useAccounts();
	const backgroundClient = useBackgroundClient();
	const createMnemonic = useMutation({
		mutationKey: ['accounts', 'v2', 'new', 'mnemonic', 'account source'],
		mutationFn: (entropy?: Uint8Array) =>
			backgroundClient.createMnemonicAccountSource({
				password: testPassNewAccounts,
				entropy: entropy ? entropyToSerialized(entropy) : undefined,
			}),
	});
	const importKey = useMutation({
		mutationKey: ['accounts', 'v2', 'import key'],
		mutationFn: ({ keyPair }: { keyPair: ExportedKeypair }) =>
			backgroundClient.createAccounts({
				type: 'imported',
				password: testPassNewAccounts,
				keyPair,
			}),
	});
	const zkCreateAccount = useMutation({
		mutationKey: ['accounts v2 create zk'],
		mutationFn: async ({ provider }: { provider: ZkProvider }) =>
			backgroundClient.createAccounts({ type: 'zk', provider }),
	});
	const [isConnectLedgerModalVisible, setIsConnectLedgerModalVisible] = useState(false);
	const networkName = useAppSelector(({ app: { apiEnv } }) => apiEnv);
	const navigate = useNavigate();
	return (
		<>
			<div className="overflow-auto h-[100vh] w-[100vw] flex flex-col items-center p-5 gap-3">
				<Popover className="relative self-stretch flex justify-center">
					<Popover.Button as="div">
						<Logo networkName={networkName} />
					</Popover.Button>
					<Popover.Panel className="absolute z-10 top-[100%] shadow-lg">
						<NetworkSelector />
					</Popover.Panel>
				</Popover>
				<div className="flex flex-col gap-10">
					<div className="grid grid-cols-2 gap-2">
						<Button
							text="Create mnemonic account source"
							loading={createMnemonic.isLoading}
							onClick={() => {
								createMnemonic.mutate(undefined);
							}}
						/>
						<Button
							text="Import mnemonic account source"
							loading={createMnemonic.isLoading}
							onClick={() => {
								createMnemonic.mutate(mnemonicToEntropy(testMnemonic));
							}}
						/>
						<Button
							text="Import random private key"
							loading={importKey.isLoading}
							onClick={() => {
								importKey.mutate({ keyPair: Ed25519Keypair.generate().export() });
							}}
						/>
						<Button
							text="Import mnemonic private key"
							loading={importKey.isLoading}
							onClick={() => {
								importKey.mutate({ keyPair: mnemonicFirstKeyPair });
							}}
						/>
						<Button
							text="Connect Ledger account"
							onClick={() => setIsConnectLedgerModalVisible(true)}
						/>
						<Button
							text="Connect Google Account"
							loading={zkCreateAccount.isLoading}
							onClick={() => zkCreateAccount.mutate({ provider: 'google' })}
						/>
					</div>
					{accounts.isLoading ? (
						<LoadingIndicator />
					) : (
						<Text>Wallet is {accounts.data?.length ? '' : 'not '}initialized</Text>
					)}
					<div className="flex flex-col gap-3">
						<Heading>Account sources</Heading>
						{accountSources.isLoading ? (
							<LoadingIndicator />
						) : accountSources.data?.length ? (
							accountSources.data.map((anAccountSource) => (
								<AccountSource key={anAccountSource.id} accountSource={anAccountSource} />
							))
						) : (
							<Text>No account sources found</Text>
						)}
					</div>
					<div className="flex flex-col gap-3">
						<Heading>Accounts</Heading>
						{accounts.isLoading ? (
							<LoadingIndicator />
						) : accounts.data?.length ? (
							accounts.data
								.sort((a, b) => {
									if (a.type !== b.type) {
										return typeOrder[a.type] - typeOrder[b.type];
									}
									if (isLedgerAccountSerializedUI(a) && isLedgerAccountSerializedUI(b)) {
										return a.derivationPath.localeCompare(b.derivationPath);
									}
									if (isMnemonicSerializedUiAccount(a) && isMnemonicSerializedUiAccount(b)) {
										if (a.sourceID === b.sourceID) {
											return a.derivationPath.localeCompare(b.derivationPath);
										}
										return a.sourceID.localeCompare(b.sourceID);
									}
									if (isQredoAccountSerializedUI(a) && isQredoAccountSerializedUI(b)) {
										if (a.sourceID === b.sourceID) {
											return a.walletID.localeCompare(b.walletID);
										}
										return a.sourceID.localeCompare(b.sourceID);
									}
									return a.address.localeCompare(b.address);
								})
								.map((anAccount) => <Account key={anAccount.id} account={anAccount} />)
						) : (
							<Text>No accounts found</Text>
						)}
					</div>
				</div>
				<Toaster
					containerClassName="z-[9999999]"
					position="bottom-right"
					toastOptions={{ success: { className: 'overflow-x-auto' } }}
				/>
			</div>
			{isConnectLedgerModalVisible ? (
				<ConnectLedgerModal
					onClose={() => setIsConnectLedgerModalVisible(false)}
					onError={(e) => toast.error(JSON.stringify(e))}
					onConfirm={() => {
						setIsConnectLedgerModalVisible(false);
						toast.success('Connect confirmed');
						navigate(
							`/accounts/import-ledger-accounts?${new URLSearchParams({
								successRedirect: '/accounts-dev',
							})}`,
						);
					}}
				/>
			) : null}
		</>
	);
}

function useLockMutation() {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationKey: ['accounts', 'v2', 'lock', 'account source or account'],
		mutationFn: async (inputs: { id: string }) =>
			backgroundClient.lockAccountSourceOrAccount(inputs),
	});
}

function useUnlockMutation() {
	const backgroundClient = useBackgroundClient();
	return useMutation({
		mutationKey: ['accounts', 'v2', 'unlock', 'account source or account'],
		mutationFn: async (inputs: Parameters<BackgroundClient['unlockAccountSourceOrAccount']>['0']) =>
			backgroundClient.unlockAccountSourceOrAccount(inputs),
	});
}

function AccountSource({ accountSource }: { accountSource: AccountSourceSerializedUI }) {
	const backgroundClient = useBackgroundClient();
	const deriveNextMnemonicAccount = useMutation({
		mutationKey: ['accounts', 'v2', 'mnemonic', 'new account'],
		mutationFn: (inputs: { sourceID: string }) =>
			backgroundClient.createAccounts({ type: 'mnemonic-derived', ...inputs }),
	});
	const lock = useLockMutation();
	const unlock = useUnlockMutation();
	return (
		<Card
			header={accountSource.id}
			key={accountSource.id}
			footer={
				accountSource.isLocked ? (
					<Button
						text="Unlock"
						onClick={() =>
							unlock.mutate({
								id: accountSource.id,
								password: testPassNewAccounts,
							})
						}
						disabled={unlock.isLoading}
					/>
				) : (
					<div className="flex gap-2 flex-1">
						<Button
							text="Lock"
							onClick={() => {
								lock.mutate({ id: accountSource.id });
							}}
							loading={lock.isLoading}
						/>
						{accountSource.type === 'mnemonic' ? (
							<Button
								text="Create next account"
								onClick={() => {
									deriveNextMnemonicAccount.mutate({ sourceID: accountSource.id });
								}}
								disabled={lock.isLoading}
								loading={deriveNextMnemonicAccount.isLoading}
							/>
						) : null}
					</div>
				)
			}
		>
			<pre>{JSON.stringify(accountSource, null, 2)}</pre>
		</Card>
	);
}

function Account({ account }: { account: SerializedUIAccount }) {
	const lock = useLockMutation();
	const unlock = useUnlockMutation();
	const signer = useSigner(account);
	const { clientIdentifier, notificationModal } = useQredoTransaction();
	const sign = useMutation({
		mutationKey: ['accounts v2 sign'],
		mutationFn: () => {
			if (account.isLocked) {
				throw new Error('Account is locked');
			}
			if (!signer) {
				throw new Error('Signer not found');
			}
			return signer.signMessage(
				{ message: new TextEncoder().encode('Message to sign') },
				clientIdentifier,
			);
		},
		onSuccess: (result) => {
			toast.success(JSON.stringify(result, null, 2));
		},
	});
	const signAndExecute = useMutation({
		mutationKey: ['accounts v2 sign'],
		mutationFn: () => {
			if (account.isLocked) {
				throw new Error('Account is locked');
			}
			if (!signer) {
				throw new Error('Signer not found');
			}
			const transactionBlock = new TransactionBlock();
			const [coin] = transactionBlock.splitCoins(transactionBlock.gas, [transactionBlock.pure(1)]);
			transactionBlock.transferObjects([coin], transactionBlock.pure(account.address));
			return signer.signAndExecuteTransactionBlock({ transactionBlock }, clientIdentifier);
		},
		onSuccess: (result) => {
			toast.success(JSON.stringify(result, null, 2));
		},
	});
	const { data: coinBalance } = useBalance(
		{ coinType: SUI_TYPE_ARG, owner: account.address },
		{ refetchInterval: 5000 },
	);
	const [formattedSuiBalance] = useFormatCoin(coinBalance?.totalBalance, coinBalance?.coinType);

	const network = useAppSelector(({ app }) => app.apiEnv);
	const faucetMutation = useFaucetMutation({
		host: network in FAUCET_HOSTS ? FAUCET_HOSTS[network as keyof typeof FAUCET_HOSTS] : null,
		address: account.address,
	});
	return (
		<>
			{notificationModal}
			<Card
				header={`${account.address} (${formattedSuiBalance} SUI)`}
				key={account.address}
				footer={
					account.isLocked ? (
						<Button
							text="Unlock"
							onClick={() => {
								unlock.mutate({ id: account.id, password: testPassNewAccounts });
							}}
							loading={unlock.isLoading}
						/>
					) : (
						<div className="flex gap-2 flex-1">
							<Button
								text="Lock"
								onClick={() => {
									lock.mutate({ id: account.id });
								}}
								loading={lock.isLoading}
							/>
							<Button
								text="Sign"
								onClick={() => {
									sign.mutate();
								}}
								loading={sign.isLoading}
								disabled={lock.isLoading || unlock.isLoading}
							/>
							<Button
								text="Sign & Execute"
								onClick={() => {
									signAndExecute.mutate();
								}}
								loading={signAndExecute.isLoading}
								disabled={lock.isLoading || unlock.isLoading}
							/>
						</div>
					)
				}
			>
				<div className="flex flex-col gap-3 items-start">
					<div>
						<Button
							text="Faucet Request"
							onClick={() => faucetMutation.mutate()}
							loading={faucetMutation.isLoading}
						/>
					</div>
					<pre>{JSON.stringify(account, null, 2)}</pre>
				</div>
			</Card>
		</>
	);
}
