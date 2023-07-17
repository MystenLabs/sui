// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { X32 } from '@mysten/icons';

import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Portal } from '../../../shared/Portal';
import { ampli } from '_src/shared/analytics/ampli';
import ExternalLink from '_src/ui/app/components/external-link';

const setInterstitialDismissed = () =>
	localStorage.setItem('bullshark-interstitial-dismissed', 'true');

const InterstitialHeading = ({ text }: { text: string }) => {
	return <div className="[-webkit-text-stroke:2px_black] text-heading1">{text}</div>;
};

const InterstitialBanner = ({ lines }: { lines: string[] }) => {
	return (
		<div className="text-heading4 flex flex-col border-solid border-2 border-gray-90 text-black bg-white py-1 px-3 rounded-md">
			{lines.map((line, key) => {
				return <div key={key}>{line}</div>;
			})}
		</div>
	);
};

function BullsharkQuestsNotification({ onClose }: { onClose: () => void }) {
	const navigate = useNavigate();

	useEffect(() => {
		const t = setTimeout(setInterstitialDismissed, 1000);
		return () => clearTimeout(t);
	}, []);

	const closeInterstitial = () => {
		setInterstitialDismissed();
		onClose();
		navigate('/apps');
	};

	return (
		<Portal containerId="overlay-portal-container">
			<div className="flex flex-col justify-center flex-nowrap items-center bg-[#99dbfb] border-solid border-4 border-black rounded-lg z-50 overflow-hidden absolute top-0 bottom-0 left-0 right-0 backdrop-blur-sm">
				<div className="flex flex-col font-frankfurter w-full h-full items-center p-4 text-white text-center">
					<div className="flex flex-col py-6 px-7 border-4 border-black border-solid w-full rounded-md h-full items-center overflow-auto">
						<div className="flex flex-col items-center">
							<InterstitialHeading text="Join Bullshark Quests!" />
							<div className="bg-[url('https://quests.mystenlabs.com/_next/static/media/logo.81b4eb8f.svg')] h-40 w-40 bg-cover" />
							<InterstitialHeading text="5 Million SUI prize pool!" />
							<div className="flex flex-col items-center gap-2 mt-2">
								<InterstitialBanner lines={['2.5M SUI', 'Top 10,000 players!']} />
								<InterstitialBanner lines={['2.5M SUI', 'all eligible players!']} />
							</div>
						</div>

						<div className="flex flex-col items-center gap-4 [-webkit-text-stroke:1px_black] w-full mt-5">
							<ExternalLink
								href="https://tech.mystenlabs.com/introducing-bullsharks-quests/"
								onClick={() => {
									ampli.clickedBullsharkQuestsCta({ sourceFlow: 'Interstitial' });
									closeInterstitial();
								}}
								className="appearance-none no-underline text-white bg-[#EA3389] rounded-lg py-2 w-60 [-webkit-text-stroke:1px_black] leading-none text-heading6"
							>
								Read more on the blog
							</ExternalLink>
							<button
								data-testid="bullshark-dismiss"
								className="appearance-none bg-transparent border-none cursor-pointer mt-1"
								onClick={closeInterstitial}
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
