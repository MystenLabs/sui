// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { UseMutationOptions } from '@tanstack/react-query';
import { useMutation } from '@tanstack/react-query';
import { useWalletContext } from 'dapp-kit/src/components/wallet-provider/WalletProvider';
import { WalletNotConnectedError } from 'dapp-kit/src/errors/walletErrors';

/**
 * Mutation hook for prompting the user to sign a message.
 */
export function useSwitchAccount(account: WalletAccount) {
	const { currentWallet, dispatch } = useWalletContext();

	dispatch('');
}
