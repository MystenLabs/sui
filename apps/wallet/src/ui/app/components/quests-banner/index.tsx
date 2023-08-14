// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import ExternalLink from '../external-link';
import { ampli } from '_src/shared/analytics/ampli';
import BullsharkOverlay from '_src/ui/assets/images/bullshark-overlay.svg';
import CapyOverlay from '_src/ui/assets/images/capy-overlay.svg';

export function QuestsBanner() {
	return (
		<div className="flex flex-col p-1 border-2 border-red border-solid mb-3 rounded-[12px] bg-brand-buttercup relative">
			<div className="flex flex-col font-arialRoundedBold w-full border-2 border-black border-solid bg-[#99DBFB] p-3 rounded-lg items-center text-white">
				<div className="text-heading6 text-black tracking-[0.42px] uppercase">Join quests 2!</div>
				<div
					className="text-[22px] font-frankfurter mt-2"
					style={{
						WebkitTextStroke: '1px black',
					}}
				>
					5 million sui prize pool
				</div>
				<ExternalLink
					href="https://tech.mystenlabs.com/bullshark-quest-2"
					onClick={() => ampli.clickedBullsharkQuestsCta({ sourceFlow: 'Banner - Apps tab' })}
					className="flex text-black appearance-none no-underline bg-brand-avocado-500 rounded-[36px] pt-2.5 pb-2 px-4 text-caption border-[3px] border-solid border-black items-center text-center mt-6 uppercase tracking-[0.42px]"
				>
					Read the blog
				</ExternalLink>
			</div>
			<div className="flex items-center justify-center absolute bottom-[-1px] left-[-1px]">
				<CapyOverlay role="img" />
			</div>
			<div className="flex items-center justify-center absolute bottom-[-1.5px] right-0">
				<BullsharkOverlay role="img" />
			</div>
		</div>
	);
}
