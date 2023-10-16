// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Search24 } from '@mysten/icons';
import { Combobox, ComboboxInput, ComboboxList } from '@mysten/ui';
import clsx from 'clsx';
import { useState, useCallback, useEffect } from 'react';
import { type Direction } from 'react-resizable-panels';

import { ModuleFunctionsInteraction } from './module-functions-interaction';
import { useBreakpoint } from '~/hooks/useBreakpoint';
import { SplitPanes } from '~/ui/SplitPanes';
import { TabHeader } from '~/ui/Tabs';
import { ListItem, VerticalList } from '~/ui/VerticalList';
import { useSearchParamsMerged } from '~/ui/utils/LinkWithQuery';
import { ModuleCodeTabs } from './ModuleCodeTabs';

type ModuleType = [moduleName: string, code: string];

interface Props {
	id: string;
	modules: ModuleType[];
	splitPanelOrientation: Direction;
}

function PkgModuleViewWrapper({ id, modules, splitPanelOrientation }: Props) {
	const isMediumOrAbove = useBreakpoint('md');

	const [searchParams, setSearchParams] = useSearchParamsMerged();
	const [query, setQuery] = useState('');

	const moduleNameValue = searchParams.get('module');
	const moduleFromParams = moduleNameValue
		? modules.find(([moduleName]) => moduleName === moduleNameValue)
		: undefined;

	// Extract module in URL or default to first module in the list
	const [selectedModuleName, selectedModuleCode] = moduleFromParams ?? modules[0];

	// If module in URL exists but is not in module list, then delete module from URL
	useEffect(() => {
		if (!moduleFromParams) {
			setSearchParams({}, { replace: true });
		}
	}, [setSearchParams, moduleFromParams]);

	const moduleNames = modules.map(([name]) => name);
	const filteredModules = query
		? moduleNames.filter(([name]) => name.toLowerCase().includes(query.toLowerCase()))
		: moduleNames;

	const submitSearch = useCallback(() => {
		if (filteredModules.length === 1) {
			setSearchParams({
				module: filteredModules[0],
			});
		}
	}, [filteredModules, setSearchParams]);

	const onChangeModule = (newModule: string) => {
		setSearchParams(
			{
				module: newModule,
			},
			{
				preventScrollReset: true,
			},
		);
	};

	const isCompact = splitPanelOrientation === 'horizontal' || !isMediumOrAbove;
	const panelContent = [
		{
			panel: (
				<ModuleCodeTabs
					packageId={id}
					moduleName={selectedModuleName}
					moduleBytecode={selectedModuleCode}
					isCompact={isCompact}
				/>
			),
			defaultSize: 40,
		},
		{
			panel: (
				<div className="h-full grow overflow-auto border-gray-45 pt-5 md:pl-7">
					<TabHeader size="md" title="Execute">
						<div className={clsx('overflow-auto', { 'h-verticalListLong': isCompact })}>
							<ModuleFunctionsInteraction
								// force recreating everything when we change modules
								key={`${id}-${selectedModuleName}`}
								packageId={id}
								moduleName={selectedModuleName}
							/>
						</div>
					</TabHeader>
				</div>
			),
			defaultSize: 60,
		},
	];

	return (
		<div className="flex flex-col gap-5 border-b border-gray-45 md:flex-row md:flex-nowrap">
			<div className="w-full md:w-1/5">
				<Combobox value={query} onValueChange={setQuery}>
					<div className="mt-2.5 flex w-full justify-between rounded-md border border-gray-50 py-1 pl-3 placeholder-gray-65 shadow-sm">
						<ComboboxInput placeholder="Search" className="w-full border-none" />
						<button onClick={submitSearch} className="border-none bg-inherit pr-2" type="submit">
							<Search24 className="h-4.5 w-4.5 cursor-pointer fill-steel align-middle text-gray-60" />
						</button>
					</div>

					<ComboboxList
						showResultsCount
						options={filteredModules.map((item) => ({ item, value: item, label: item }))}
						onSelect={({ item }) => {
							onChangeModule(item);
						}}
					/>
				</Combobox>
				<div className="h-verticalListShort overflow-auto pt-3 md:h-verticalListLong">
					<VerticalList>
						{moduleNames.map((name) => (
							<div key={name} className="mx-0.5 mt-0.5 md:min-w-fit">
								<ListItem active={selectedModuleName === name} onClick={() => onChangeModule(name)}>
									{name}
								</ListItem>
							</div>
						))}
					</VerticalList>
				</div>
			</div>
			{isMediumOrAbove ? (
				<div className="w-4/5">
					<SplitPanes direction={splitPanelOrientation} splitPanels={panelContent} />
				</div>
			) : (
				panelContent.map((panel, index) => <div key={index}>{panel.panel}</div>)
			)}
		</div>
	);
}
export default PkgModuleViewWrapper;
