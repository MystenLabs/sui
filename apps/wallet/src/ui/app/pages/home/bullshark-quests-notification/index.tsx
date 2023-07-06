// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { X32 } from '@mysten/icons';
import cl from 'classnames';
import { set } from 'idb-keyval';
import { useNavigate } from 'react-router-dom';

import { Portal } from '../../../shared/Portal';

import st from './Overlay.module.scss';

function BullsharkQuestsNotification() {
	const navigate = useNavigate();

	const setInterstitialDismissed = async () => {
		await set('bullshark-interstitial-dismissed', true);
	};

	return (
		<Portal containerId="overlay-portal-container">
			<div className={cl(st.container)}>
				<div className={st.content}>
					<div className="flex flex-col font-frankfurter w-full h-full items-center p-4">
						<div className="flex flex-col p-2 border-4 border-black border-solid w-full rounded-md justify-between h-full items-center">
							<div className="flex flex-col items-center">
								<div className="font-frankfurter">Join Bullshark Quests!</div>
								<div className="bg-[url('https://quests.mystenlabs.com/_next/static/media/logo.81b4eb8f.svg')] h-50 w-50 bg-cover" />
								<div>5 Million SUI prize pool!</div>
								<div>2.5M SUI Top 10,000 players!</div>
								<div>2.5M SUI Top all eligible players!</div>
							</div>

							<div>Read more on the blog</div>
							<button
								className="appearance-none bg-transparent border-none cursor-pointer"
								onClick={() => {
									setInterstitialDismissed();
									navigate('/tokens');
								}}
							>
								<X32 className="text-sui-dark h-8 w-8" />
							</button>
						</div>
					</div>
				</div>
			</div>
		</Portal>
	);
}

export default BullsharkQuestsNotification;
