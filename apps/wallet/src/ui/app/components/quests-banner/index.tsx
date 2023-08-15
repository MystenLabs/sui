// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import ExternalLink from '../external-link';
import { ampli } from '_src/shared/analytics/ampli';
import Quests2Banner from '_src/ui/assets/images/quests-2-banner.svg';

export function QuestsBanner() {
	return (
		<div className="mb-3">
			<ExternalLink
				href="https://tech.mystenlabs.com/bullshark-quest-2"
				onClick={() => ampli.clickedBullsharkQuestsCta({ sourceFlow: 'Banner - Apps tab' })}
			>
				<Quests2Banner role="img" />
			</ExternalLink>
		</div>
	);
}
