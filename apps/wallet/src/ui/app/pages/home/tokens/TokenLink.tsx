// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { MIST_PER_SUI, type CoinBalance } from '@mysten/sui.js';
import { type ReactNode } from 'react';
import { Link } from 'react-router-dom';

import { CoinItem } from '_components/active-coins-card/CoinItem';
import { ampli } from '_src/shared/analytics/ampli';

type Props = {
	coinBalance: CoinBalance;
	centerAction?: ReactNode;
};

export function TokenLink({ coinBalance, centerAction }: Props) {
	return (
		<Link
			to={`/send?type=${encodeURIComponent(coinBalance.coinType)}`}
			onClick={() =>
				ampli.selectedCoin({
					coinType: coinBalance.coinType,
					totalBalance: Number(BigInt(coinBalance.totalBalance) / MIST_PER_SUI),
				})
			}
			key={coinBalance.coinType}
			className="no-underline w-full group/coin"
		>
			<CoinItem
				coinType={coinBalance.coinType}
				balance={BigInt(coinBalance.totalBalance)}
				centerAction={centerAction}
			/>
		</Link>
	);
}
