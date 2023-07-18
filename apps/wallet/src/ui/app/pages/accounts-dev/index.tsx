// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMutation, useQueryClient } from '@tanstack/react-query';
import LoadingIndicator from '../../components/loading/LoadingIndicator';
import {
	accountSourcesQueryKey,
	useAccountSources,
} from '../../hooks/accounts-v2/useAccountSources';
import { accountsQueryKey, useAccounts } from '../../hooks/accounts-v2/useAccounts';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { Button } from '../../shared/ButtonUI';
import { Card } from '../../shared/card';
import { Heading } from '../../shared/heading';
import { Text } from '../../shared/text';
import { type AccountSourceType } from '_src/background/account-sources/AccountSource';
import { entropyToSerialized, mnemonicToEntropy } from '_src/shared/utils/bip39';

const pass = '61916a448d7885641';
const testMnemonic =
	'lawsuit welcome deputy faith shadow monitor common paper candy horse panda history';

/**
 * Just for dev, to simplify testing new accounts handling
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
	const unlockAccountSource = useMutation({
		mutationKey: ['accounts', 'v2', 'unlock', 'account source'],
		mutationFn: async (inputs: { id: string; type: AccountSourceType; password: string }) =>
			backgroundClient.unlockAccountSource(inputs),
		onSuccess: () => {
			queryClient.invalidateQueries({ exact: true, queryKey: accountSourcesQueryKey });
		},
	});
	const deriveNextMnemonicAccount = useMutation({
		mutationKey: ['accounts', 'v2', 'mnemonic', 'new account'],
		mutationFn: (inputs: { sourceID: string }) => backgroundClient.deriveMnemonicAccount(inputs),
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
							{accountSources.data?.length ? (
								accountSources.data.map((anAccountSource) => (
									<Card
										header={anAccountSource.id}
										key={anAccountSource.id}
										footer={
											anAccountSource.isLocked ? (
												<Button
													size="tiny"
													text="Unlock"
													onClick={() =>
														unlockAccountSource.mutate({
															id: anAccountSource.id,
															type: anAccountSource.type,
															password: pass,
														})
													}
													disabled={unlockAccountSource.isLoading}
												/>
											) : (
												<div className="flex gap-2">
													<Button text="Lock" />
													<Button
														text="Create next account"
														onClick={() => {
															deriveNextMnemonicAccount.mutate({ sourceID: anAccountSource.id });
														}}
													/>
												</div>
											)
										}
									>
										<pre>{JSON.stringify(anAccountSource, null, 2)}</pre>
									</Card>
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
						accounts.data.map((anAccount) => (
							<Card
								header={anAccount.address}
								key={anAccount.address}
								footer={
									anAccount.isLocked ? (
										<Button
											size="tiny"
											text="Unlock"
											onClick={() => {
												//TODO
											}}
											disabled={false}
										/>
									) : (
										<div className="flex gap-2">
											<Button text="Lock" />
											<Button
												text="Sign"
												onClick={() => {
													// TODO
												}}
											/>
										</div>
									)
								}
							>
								<pre>{JSON.stringify(anAccount, null, 2)}</pre>
							</Card>
						))
					) : (
						<Text>No accounts found</Text>
					)}
				</div>
			</div>
		</div>
	);
}
