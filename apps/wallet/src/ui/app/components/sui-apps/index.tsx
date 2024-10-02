// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Heading } from '_app/shared/heading';
import { Text } from '_app/shared/text';
import { useAppSelector } from '_hooks';
import { FEATURES } from '_src/shared/experimentation/features';
import { prepareLinkToCompare } from '_src/shared/utils';
import { useFeature } from '@growthbook/growthbook-react';
import { useMemo } from 'react';
import { useParams } from 'react-router-dom';

import { permissionsSelectors } from '../../redux/slices/permissions';
import { AppsPageBanner } from './Banner';
import { SuiApp, type DAppEntry } from './SuiApp';
import { SuiAppEmpty } from './SuiAppEmpty';

function AppsPlayGround() {
	const ecosystemApps = useFeature<DAppEntry[]>(FEATURES.WALLET_DAPPS).value;
	const { tagName } = useParams();

	const filteredEcosystemApps = useMemo(() => {
		if (!ecosystemApps) {
			return [];
		} else if (tagName) {
			return ecosystemApps.filter((app) => app.tags.includes(tagName));
		}
		return ecosystemApps;
	}, [ecosystemApps, tagName]);

	const allPermissions = useAppSelector(permissionsSelectors.selectAll);
	const linkToPermissionID = useMemo(() => {
		const map = new Map<string, string>();
		for (const aPermission of allPermissions) {
			map.set(prepareLinkToCompare(aPermission.origin), aPermission.id);
			if (aPermission.pagelink) {
				map.set(prepareLinkToCompare(aPermission.pagelink), aPermission.id);
			}
		}
		return map;
	}, [allPermissions]);

	return (
		<>
			<div className="flex justify-center mb-4">
				<Heading variant="heading6" color="gray-90" weight="semibold">
					Sui Apps
				</Heading>
			</div>

			<AppsPageBanner />

			{filteredEcosystemApps?.length ? (
				<div className="p-4 bg-gray-40 rounded-xl">
					<Text variant="pBodySmall" color="gray-75" weight="normal">
						Apps below are actively curated but do not indicate any endorsement or relationship with
						Sui Wallet. Please DYOR.
					</Text>
				</div>
			) : null}

			{filteredEcosystemApps?.length ? (
				<div className="flex flex-col divide-y divide-gray-45 divide-solid divide-x-0 mt-2">
					{filteredEcosystemApps.map((app) => (
						<SuiApp
							key={app.link}
							{...app}
							permissionID={linkToPermissionID.get(prepareLinkToCompare(app.link))}
							displayType="full"
							openAppSite
						/>
					))}
				</div>
			) : (
				<SuiAppEmpty displayType="full" />
			)}
		</>
	);
}

export default AppsPlayGround;
export { default as ConnectedAppsCard } from './ConnectedAppsCard';
