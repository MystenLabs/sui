// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureIsOn } from '@growthbook/growthbook-react';
import {
	ConnectButton,
	useWalletKit,
	type StandardWalletAdapter,
	type WalletWithFeatures,
} from '@mysten/wallet-kit';
import { useParams } from 'react-router-dom';

import { Button } from '~/ui/Button';

// This is a custom feature supported by the Sui Wallet:
type StakeInput = { validatorAddress: string };
type SuiWalletStakeFeature = {
	'suiWallet:stake': {
		version: '0.0.1';
		stake: (input: StakeInput) => Promise<void>;
	};
};

type StakeWallet = WalletWithFeatures<Partial<SuiWalletStakeFeature>>;

export function StakeButton() {
	const stakeButtonEnabled = useFeatureIsOn('validator-page-staking');
	const { id } = useParams();
	const { wallets, currentWallet, connect } = useWalletKit();

	if (!stakeButtonEnabled) return null;

	const stakeSupportedWallets = wallets.filter((wallet) => {
		if (!('wallet' in wallet)) {
			return false;
		}

		const standardWallet = wallet.wallet as StakeWallet;
		return 'suiWallet:stake' in standardWallet.features;
	});

	const currentWalletSupportsStake =
		currentWallet && !!stakeSupportedWallets.find(({ name }) => currentWallet.name === name);

	if (!stakeSupportedWallets.length) {
		return (
			<Button
				size="lg"
				href="https://chrome.google.com/webstore/detail/sui-wallet/opcgpfmipidbgpenhmajoajpbobppdil"
			>
				Install Sui Wallet to stake SUI
			</Button>
		);
	}

	if (!currentWallet) {
		return (
			<ConnectButton
				className="!border !border-solid !border-steel-dark !bg-transparent !px-4 !py-3 !text-body !font-semibold !text-steel-dark !shadow-none"
				connectText="Stake SUI"
			/>
		);
	}

	if (!currentWalletSupportsStake) {
		return (
			<Button
				size="lg"
				onClick={() => {
					// Always just assume we should connect to the first stake supported wallet for now:
					connect(stakeSupportedWallets[0].name);
				}}
			>
				Stake SUI on a supported wallet
			</Button>
		);
	}

	return (
		<Button
			size="lg"
			onClick={() => {
				((currentWallet as StandardWalletAdapter).wallet as StakeWallet).features[
					'suiWallet:stake'
				]?.stake({ validatorAddress: id! });
			}}
		>
			Stake SUI
		</Button>
	);
}
