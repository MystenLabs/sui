// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { X32 } from '@mysten/icons';

import { cva, type VariantProps } from 'class-variance-authority';

import { type ReactNode, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Portal } from '../../../shared/Portal';
import { ampli } from '_src/shared/analytics/ampli';
import ExternalLink from '_src/ui/app/components/external-link';
import CharactersIcon from '_src/ui/assets/images/characters-icon.svg';
import Checkmark from '_src/ui/assets/images/quests-checkmark.svg';

const textStyles = cva([], {
	variants: {
		stroke: {
			lg: '2.5px black',
			md: '2px black',
		},
		variant: {
			lg: 'text-[36px] tracking-[-0.36px]',
			md: 'text-heading1 tracking-[-0.32px]',
			sm: 'text-heading2',
		},
	},
});

type TextStylesProps = VariantProps<typeof textStyles>;

export interface TextProps extends TextStylesProps {
	children: ReactNode;
}

export function InterstitialText({ children, stroke, ...styleProps }: TextProps) {
	return (
		<div
			className={textStyles({ ...styleProps })}
			style={{
				WebkitTextStroke: textStyles({ stroke }),
			}}
		>
			{children}
		</div>
	);
}

const setInterstitialDismissed = () =>
	localStorage.setItem('bullshark-interstitial-dismissed', 'true');

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
			<div className="flex flex-col justify-center flex-nowrap items-center border-solid border-4 border-black rounded-lg z-50 overflow-hidden absolute top-0 bottom-0 left-0 right-0 backdrop-blur-sm">
				<div className="flex flex-col font-frankfurter w-full h-full items-center bg-brand-buttercup p-3 text-white text-center">
					<div className="flex flex-col py-6 px-7 border-4 border-black border-solid w-full rounded-md h-full items-center overflow-auto bg-gradient-to-b from-[#99dbfb] to-white">
						<div className="flex flex-col items-center justify-between h-full">
							<div className="flex flex-col">
								<div className="flex flex-col">
									<InterstitialText stroke="lg" variant="lg">
										Join Quests 2!
									</InterstitialText>
									<div className="text-black text-heading6 font-normal font-arialRoundedBold mt-1">
										Capy joins the Quests!
									</div>
								</div>

								<div className="flex items-center justify-center h-40 w-full my-8">
									<CharactersIcon role="img" />
								</div>
								<InterstitialText variant="sm" stroke="md">
									5 Million SUI
									<br />
									prize pool
									<div className="h-0.5 bg-black w-full my-3" />
								</InterstitialText>
								<div className="flex w-full text-black gap-3 text-left font-arialRoundedBold text-body">
									<div className="min-w-7">
										<Checkmark width={28} height={27} role="img" />
									</div>
									<div className="w-full">2.5M SUI for top 5,000 players</div>
									<div className="h-full bg-black min-w-[2px]"></div>
									<div className="min-w-7">
										<Checkmark width={28} height={27} role="img" />
									</div>
									<div className="w-full">2.5M SUI for all eligible players</div>
								</div>
							</div>

							<div className="flex flex-col items-center gap-2 [-webkit-text-stroke:1px_black] w-full mt-3">
								<ExternalLink
									href="https://tech.mystenlabs.com/bullshark-quest-2"
									onClick={() => {
										ampli.clickedBullsharkQuestsCta({ sourceFlow: 'Interstitial' });
										closeInterstitial();
									}}
									className="appearance-none no-underline text-white bg-brand-avocado-500 rounded-[36px] py-2 w-full [-webkit-text-stroke:1px_black] leading-none text-heading6 border-[3px] border-solid border-black"
								>
									<InterstitialText variant="md" stroke="md">
										Read the blog
									</InterstitialText>
								</ExternalLink>
								<button
									data-testid="bullshark-dismiss"
									className="appearance-none bg-transparent border-none cursor-pointer"
									onClick={closeInterstitial}
								>
									<X32 className="text-black h-8 w-8" />
								</button>
							</div>
						</div>
					</div>
				</div>
			</div>
		</Portal>
	);
}

export default BullsharkQuestsNotification;
