// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Ed25519Keypair, type ExportedKeypair, toB64 } from '@mysten/sui.js';
import { hexToBytes } from '@noble/hashes/utils';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Toaster, toast } from 'react-hot-toast';
import { type BackgroundClient } from '../../background-client';
import LoadingIndicator from '../../components/loading/LoadingIndicator';
import {
	accountSourcesQueryKey,
	useAccountSources,
} from '../../hooks/accounts-v2/useAccountSources';
import { accountsQueryKey, useAccounts } from '../../hooks/accounts-v2/useAccounts';
import { useSigner } from '../../hooks/accounts-v2/useSigner';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Button } from '../../shared/ButtonUI';
import { Card } from '../../shared/card';
import { Heading } from '../../shared/heading';
import { Text } from '../../shared/text';
import { type AccountSourceSerializedUI } from '_src/background/account-sources/AccountSource';
import { type SerializedUIAccount } from '_src/background/accounts/Account';
import { entropyToSerialized, mnemonicToEntropy } from '_src/shared/utils/bip39';

const pass = '61916a448d7885641';
const testMnemonic =
	'lawsuit welcome deputy faith shadow monitor common paper candy horse panda history';
const mnemonicFirstKeyPair: ExportedKeypair = {
	schema: 'ED25519',
	privateKey: toB64(hexToBytes('5051bc918ec4991c62969d6cd0f1edaabfbe5244e509d7a96f39fe52e76cf54f')),
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
				password: pass,
				entropy: entropy ? entropyToSerialized(entropy) : undefined,
			}),
		onSuccess: () => {
			queryClient.invalidateQueries({ exact: true, queryKey: accountSourcesQueryKey });
		},
	});
	const importKey = useMutation({
		mutationKey: ['accounts', 'v2', 'import key'],
		mutationFn: ({ keyPair }: { keyPair: ExportedKeypair }) =>
			backgroundClient.createAccount({
				type: 'imported',
				password: pass,
				keyPair,
			}),
		onSuccess: () => {
			queryClient.invalidateQueries({ exact: true, queryKey: accountsQueryKey });
		},
	});
	return (
		<div className="overflow-auto h-[100vh] w-[100vw] flex flex-col items-center p-5">
			<div className="flex flex-col gap-10">
				{accounts.isLoading ? (
					<LoadingIndicator />
				) : (
					<Text>Wallet is {accounts.data?.length ? '' : 'not '}initialized</Text>
				)}
				<div className="flex flex-col gap-3">
					<Heading>Account sources</Heading>
					{accountSources.isLoading ? (
						<LoadingIndicator />
					) : (
						<>
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
							{accountSources.data?.length ? (
								accountSources.data.map((anAccountSource) => (
									<AccountSource key={anAccountSource.id} accountSource={anAccountSource} />
								))
							) : (
								<Text>No account sources found</Text>
							)}
						</>
					)}
				</div>
				<div className="flex flex-col gap-3">
					<Heading>Accounts</Heading>
					{accounts.isLoading ? (
						<LoadingIndicator />
					) : accounts.data?.length ? (
						accounts.data.map((anAccount) => <Account key={anAccount.id} account={anAccount} />)
					) : (
						<Text>No accounts found</Text>
					)}
				</div>
			</div>
			<Toaster
				position="bottom-right"
				toastOptions={{ success: { className: 'overflow-x-auto' } }}
			/>
		</div>
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
			backgroundClient.createAccount({ type: 'mnemonic-derived', ...inputs }),
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
						size="tiny"
						text="Unlock"
						onClick={() =>
							unlock.mutate({
								id: accountSource.id,
								password: pass,
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
						<Button
							text="Create next account"
							onClick={() => {
								deriveNextMnemonicAccount.mutate({ sourceID: accountSource.id });
							}}
							disabled={lock.isLoading}
							loading={deriveNextMnemonicAccount.isLoading}
						/>
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
	const sign = useMutation({
		mutationKey: ['accounts v2 sign'],
		mutationFn: () => {
			if (account.isLocked) {
				throw new Error('Account is locked');
			}
			if (!signer) {
				throw new Error('Signer not found');
			}
			return signer.signMessage({ message: new TextEncoder().encode('Message to sign') });
		},
		onSuccess: (result) => {
			toast.success(JSON.stringify(result, null, 2));
		},
	});
	return (
		<Card
			header={account.address}
			key={account.address}
			footer={
				account.isLocked ? (
					<Button
						text="Unlock"
						onClick={() => {
							unlock.mutate({ id: account.id, password: pass });
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
	);
}
