// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ExportedKeypair } from '@mysten/sui.js/cryptography';
import { Ed25519Keypair } from '@mysten/sui.js/keypairs/ed25519';
import { toB64 } from '@mysten/sui.js/utils';
import { hexToBytes } from '@noble/hashes/utils';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { Toaster, toast } from 'react-hot-toast';
import { type BackgroundClient } from '../../background-client';
import { ConnectLedgerModal } from '../../components/ledger/ConnectLedgerModal';
import LoadingIndicator from '../../components/loading/LoadingIndicator';
import {
	accountSourcesQueryKey,
	useAccountSources,
} from '../../hooks/accounts-v2/useAccountSources';
import { accountsQueryKey, useAccounts } from '../../hooks/accounts-v2/useAccounts';
import { useSigner } from '../../hooks/accounts-v2/useSigner';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { useQredoTransaction } from '../../hooks/useQredoTransaction';
import { ImportLedgerAccountsPage } from '../../pages/accounts/ImportLedgerAccountsPage';
import { Button } from '../../shared/ButtonUI';
import { ModalDialog } from '../../shared/ModalDialog';
import { Card } from '../../shared/card';
import { Heading } from '../../shared/heading';
import { Text } from '../../shared/text';
import { type AccountSourceSerializedUI } from '_src/background/account-sources/AccountSource';
import { type AccountType, type SerializedUIAccount } from '_src/background/accounts/Account';
import { isLedgerAccountSerializedUI } from '_src/background/accounts/LedgerAccount';
import { isMnemonicSerializedUiAccount } from '_src/background/accounts/MnemonicAccount';
import { isQredoAccountSerializedUI } from '_src/background/accounts/QredoAccount';
import { entropyToSerialized, mnemonicToEntropy } from '_src/shared/utils/bip39';

export const testPassNewAccounts = '61916a448d7885641';
const testMnemonic =
	'lawsuit welcome deputy faith shadow monitor common paper candy horse panda history';
const mnemonicFirstKeyPair: ExportedKeypair = {
	schema: 'ED25519',
	privateKey: toB64(hexToBytes('5051bc918ec4991c62969d6cd0f1edaabfbe5244e509d7a96f39fe52e76cf54f')),
};
const typeOrder: Record<AccountType, number> = {
	'mnemonic-derived': 0,
	imported: 1,
	ledger: 2,
	qredo: 3,
};

/**
 * Just for dev, to allow testing new accounts handling
 */
export function AccountsDev() {
	const accountSources = useAccountSources();
	const accounts = useAccounts();
	const backgroundClient = useBackgroundClient();
	const queryClient = useQueryClient();
	const createMnemonic = useMutation({
		mutationKey: ['accounts', 'v2', 'new', 'mnemonic', 'account source'],
		mutationFn: (entropy?: Uint8Array) =>
			backgroundClient.createMnemonicAccountSource({
				password: testPassNewAccounts,
				entropy: entropy ? entropyToSerialized(entropy) : undefined,
			}),
		onSuccess: () => {
			queryClient.invalidateQueries({ exact: true, queryKey: accountSourcesQueryKey });
		},
	});
	const importKey = useMutation({
		mutationKey: ['accounts', 'v2', 'import key'],
		mutationFn: ({ keyPair }: { keyPair: ExportedKeypair }) =>
			backgroundClient.createAccounts({
				type: 'imported',
				password: testPassNewAccounts,
				keyPair,
			}),
		onSuccess: () => {
			queryClient.invalidateQueries({ exact: true, queryKey: accountsQueryKey });
		},
	});
	const [isConnectLedgerModalVisible, setIsConnectLedgerModalVisible] = useState(false);
	const [isImportLedgerModalVisible, setIsImportLedgerModalVisible] = useState(false);
	return (
		<>
			<div className="overflow-auto h-[100vh] w-[100vw] flex flex-col items-center p-5">
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
						setIsImportLedgerModalVisible(true);
						toast.success('Connect confirmed');
					}}
				/>
			) : null}
			<ModalDialog
				isOpen={isImportLedgerModalVisible}
				onClose={() => setIsImportLedgerModalVisible(false)}
				body={
					<>
						<div id="overlay-portal-container"></div>
						<ImportLedgerAccountsPage
							password={testPassNewAccounts}
							onClose={() => setIsImportLedgerModalVisible(false)}
							onConfirmed={() => {
								setIsImportLedgerModalVisible(false);
								queryClient.invalidateQueries({ exact: true, queryKey: accountsQueryKey });
							}}
						/>
					</>
				}
			/>
		</>
	);
}

function useLockMutation() {
	const backgroundClient = useBackgroundClient();
	const queryClient = useQueryClient();
	return useMutation({
		mutationKey: ['accounts', 'v2', 'lock', 'account source or account'],
		mutationFn: async (inputs: { id: string }) =>
			backgroundClient.lockAccountSourceOrAccount(inputs),
		onSuccess: () => {
			queryClient.invalidateQueries({ exact: true, queryKey: accountSourcesQueryKey });
			queryClient.invalidateQueries({ exact: true, queryKey: accountsQueryKey });
		},
	});
}

function useUnlockMutation() {
	const backgroundClient = useBackgroundClient();
	const queryClient = useQueryClient();
	return useMutation({
		mutationKey: ['accounts', 'v2', 'unlock', 'account source or account'],
		mutationFn: async (inputs: Parameters<BackgroundClient['unlockAccountSourceOrAccount']>['0']) =>
			backgroundClient.unlockAccountSourceOrAccount(inputs),
		onSuccess: () => {
			queryClient.invalidateQueries({ exact: true, queryKey: accountSourcesQueryKey });
			queryClient.invalidateQueries({ exact: true, queryKey: accountsQueryKey });
		},
	});
}

function AccountSource({ accountSource }: { accountSource: AccountSourceSerializedUI }) {
	const backgroundClient = useBackgroundClient();
	const queryClient = useQueryClient();
	const deriveNextMnemonicAccount = useMutation({
		mutationKey: ['accounts', 'v2', 'mnemonic', 'new account'],
		mutationFn: (inputs: { sourceID: string }) =>
			backgroundClient.createAccounts({ type: 'mnemonic-derived', ...inputs }),
		onSuccess: () => {
			queryClient.invalidateQueries({ exact: true, queryKey: accountsQueryKey });
		},
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
	return (
		<>
			{notificationModal}
			<Card
				header={account.address}
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
						</div>
					)
				}
			>
				<pre>{JSON.stringify(account, null, 2)}</pre>
			</Card>
		</>
	);
}
