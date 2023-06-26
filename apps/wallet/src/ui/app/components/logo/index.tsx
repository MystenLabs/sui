// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Sui, SuiLogoTxt } from '@mysten/icons';

import { Text } from '../../shared/text';
import { type API_ENV, networkNames } from '_src/shared/api-env';

type LogoProps = {
	networkName?: API_ENV;
};

const Logo = ({ networkName }: LogoProps) => {
	return (
		<div className="inline-flex flex-nowrap items-center gap-0.5 text-gray-90">
			<Sui className="h-[26px] w-5" />
			<div className="flex flex-col">
				<SuiLogoTxt className="w-5 h-[13px]" />
				{networkName && (
					<div className="whitespace-nowrap">
						<Text variant="subtitleSmallExtra">{networkNames[networkName]}</Text>
					</div>
				)}
			</div>
		</div>
	);
};

export default Logo;
