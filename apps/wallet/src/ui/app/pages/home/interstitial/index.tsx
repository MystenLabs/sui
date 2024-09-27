// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ButtonOrLink } from '_app/shared/utils/ButtonOrLink';
import { ampli } from '_src/shared/analytics/ampli';
import ExternalLink from '_src/ui/app/components/external-link';
import { Text } from '_src/ui/app/shared/text';
import { X32 } from '@mysten/icons';
import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

import { Portal } from '../../../shared/Portal';

export type InterstitialConfig = {
	enabled: boolean;
	dismissKey?: string;
	imageUrl?: string;
	buttonText?: string;
	bannerUrl?: string;
};

interface InterstitialProps extends InterstitialConfig {
	onClose: () => void;
}

const setInterstitialDismissed = (dismissKey: string) => localStorage.setItem(dismissKey, 'true');

function Interstitial({
	enabled,
	dismissKey,
	imageUrl,
	bannerUrl,
	buttonText,
	onClose,
}: InterstitialProps) {
	const navigate = useNavigate();

	useEffect(() => {
		const t = setTimeout(setInterstitialDismissed, 1000);
		return () => clearTimeout(t);
	}, []);

	const closeInterstitial = (dismissKey?: string) => {
		if (dismissKey) {
			setInterstitialDismissed(dismissKey);
		}
		onClose();
		navigate('/apps');
	};

	const onClick = () => {
		ampli.clickedBullsharkQuestsCta({ sourceFlow: 'Interstitial' });
		closeInterstitial();
	};

	if (!enabled) {
		return null;
	}

	return (
		<Portal containerId="overlay-portal-container">
			<div className="flex flex-col flex-nowrap items-center rounded-lg z-50 overflow-hidden absolute top-0 bottom-0 left-0 right-0 backdrop-blur-sm bg-[rgba(17,35,55,.56)] py-8 justify-between h-full">
				<button
					data-testid="bullshark-dismiss"
					className="appearance-none bg-transparent border-none cursor-pointer w-full"
					onClick={() => closeInterstitial(dismissKey)}
				>
					<X32 className="text-white h-8 w-8" />
				</button>
				{bannerUrl && (
					<ExternalLink href={bannerUrl} onClick={onClick} className="w-full text-center">
						<img className="rounded-2xl w-full p-2" src={imageUrl} alt="interstitial-banner" />
					</ExternalLink>
				)}
				<ButtonOrLink
					className="flex appearance-none border-none rounded-full bg-[#4CA2FF] h-10 px-6 cursor-pointer items-center no-underline"
					onClick={onClick}
					to={bannerUrl}
					target="_blank"
					rel="noreferrer noopener"
				>
					<Text variant="body" weight="semibold" color="white">
						{buttonText || 'Join for a chance to win'}
					</Text>
				</ButtonOrLink>
			</div>
		</Portal>
	);
}

export default Interstitial;
