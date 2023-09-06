// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '@mysten/ui';

import {
	SUI_0x1,
	SUI_0x2,
	type SuiDependency,
	type SuiPackageVersion,
} from '~/components/module/dependencyUtils';
import { ProgrammableTxnBlockCard } from '~/components/transactions/ProgTxnBlockCard';
import { ObjectLink } from '~/ui/InternalLink';
import { TransactionBlockCardSection } from '~/ui/TransactionBlockCard';
const DEFAULT_ITEMS_TO_SHOW = 10;

interface DependenciesCardProps {
	suiDependencies?: SuiDependency[];
	itemsLabel?: string;
	defaultOpen?: boolean;
}
interface SmallCardSectionProps {
	title: string;
	suiPackageInfo: SuiPackageVersion;
}

function SmallCardSection({ title, suiPackageInfo }: SmallCardSectionProps) {
	return (
		<TransactionBlockCardSection key={title} title={title} defaultOpen>
			<div data-testid="small-inputs-card-content" className="flex flex-col gap-2">
				<div className="flex items-start justify-between">
					<Text variant="pBody/medium" color="steel-dark">
						Package ID
					</Text>
					<div className="min-w-[140px] break-all text-right">
						<Text variant="pBody/medium" color="steel-darker">
							{suiPackageInfo.packageId ? (
								<ObjectLink objectId={suiPackageInfo.packageId} />
							) : (
								'N/A'
							)}
						</Text>
					</div>
				</div>

				<div className="flex items-start justify-between">
					<Text variant="pBody/medium" color="steel-dark">
						Version
					</Text>
					<div className="min-w-[140px] break-all text-right">
						<Text variant="pBody/medium" color="steel-darker">
							{suiPackageInfo.version || 'N/A'}
						</Text>
					</div>
				</div>
			</div>
		</TransactionBlockCardSection>
	);
}

export function DependenciesCard({
	suiDependencies,
	itemsLabel = 'Dependencies',
	defaultOpen = false,
}: DependenciesCardProps) {
	if (!suiDependencies?.length) {
		return null;
	}

	const expandableItems = suiDependencies.map((suiDependency, index) => (
		<TransactionBlockCardSection
			key={index}
			title={`Dependency ${index + 1}`}
			defaultOpen={defaultOpen}
		>
			<div data-testid="inputs-card-content" className="flex flex-col gap-2">
				<div
					key={`dependency-upgrade-cap-id-${suiDependency.upgradeCapId}`}
					className="flex items-start justify-between"
				>
					<Text variant="pBody/medium" color="steel-dark">
						UpgradeCap ID
					</Text>

					<div className="max-w-[66%] break-all text-right">
						<Text variant="pBody/medium" color="steel-darker">
							{getUpgradeCapIdLink(suiDependency.orgPackageId, suiDependency.upgradeCapId)}
						</Text>
					</div>
				</div>
				<div
					key={`dependency-org-package-id-${suiDependency.orgPackageId}`}
					className="flex items-start justify-between"
				>
					<Text variant="pBody/medium" color="steel-dark">
						Original Package ID
					</Text>

					<div className="max-w-[66%] break-all text-right">
						<Text variant="pBody/medium" color="steel-darker">
							<ObjectLink objectId={suiDependency.orgPackageId} />
						</Text>
					</div>
				</div>
				<div
					key={`dependency-current-${suiDependency.current}`}
					className="flex items-start justify-between"
				>
					<Text variant="pBody/medium" color="steel-dark">
						Current
					</Text>

					<div className="max-w-[66%] break-all text-right">
						<Text variant="pBody/medium" color="steel-darker">
							<SmallCardSection title="" suiPackageInfo={suiDependency.current} />
						</Text>
					</div>
				</div>
				<div
					key={`dependency-latest-${suiDependency.latest}`}
					className="flex items-start justify-between"
				>
					<Text variant="pBody/medium" color="steel-dark">
						Latest
					</Text>

					<div className="max-w-[66%] break-all text-right">
						<Text variant="pBody/medium" color="steel-darker">
							<SmallCardSection title="" suiPackageInfo={suiDependency.latest} />
						</Text>
					</div>
				</div>
			</div>
		</TransactionBlockCardSection>
	));

	return (
		<ProgrammableTxnBlockCard
			items={expandableItems}
			itemsLabel={itemsLabel}
			defaultItemsToShow={DEFAULT_ITEMS_TO_SHOW}
			noExpandableList={defaultOpen}
		/>
	);
}

function getUpgradeCapIdLink(packageId: string, upgradeCapId: string | null) {
	if (isSuiFrameworkId(packageId)) {
		return 'N/A';
	}

	if (!upgradeCapId) {
		return 'Deleted';
	}

	return <ObjectLink objectId={upgradeCapId} />;
}

function isSuiFrameworkId(objectId: string) {
	return objectId === SUI_0x1 || objectId === SUI_0x2;
}
