// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Search24 } from '@mysten/icons';
import { Combobox, ComboboxInput, ComboboxList } from '@mysten/ui';
import axios, { type AxiosResponse } from 'axios';
import clsx from 'clsx';
import JSZip from 'jszip';
import { useCallback, useEffect, useState } from 'react';
import { type Direction } from 'react-resizable-panels';

import ModuleView from './ModuleView';
import { ModuleFunctionsInteraction } from './module-functions-interaction';
import { VerificationApiEndpoint } from '~/components/module/VerificationApiEndpoint';
import VerifiedModuleViewWrapper from '~/components/module/VerifiedModuleViewWrapper';
import { type SuiVerificationCheckResultDto } from '~/components/module/dto/verification/SuiVerificationCheckResultDto';
import { useNetwork } from '~/context';
import { useBreakpoint } from '~/hooks/useBreakpoint';
import { SplitPanes } from '~/ui/SplitPanes';
import { TabHeader, Tabs, TabsContent, TabsList, TabsTrigger } from '~/ui/Tabs';
import { ListItem, VerticalList } from '~/ui/VerticalList';
import { useSearchParamsMerged } from '~/ui/utils/LinkWithQuery';

export type ModuleType = [moduleName: string, code: string];

export interface PackageFile {
	relativePath: string;
	content: string;
}

interface Props {
	id?: string;
	modules: ModuleType[];
	splitPanelOrientation: Direction;
	initialTab?: string | null;
}

interface ModuleViewWrapperProps {
	id?: string;
	selectedModuleName: string;
	modules: ModuleType[];
}

export interface VerificationResult {
	isVerified: boolean;
}

function ModuleViewWrapper({ id, selectedModuleName, modules }: ModuleViewWrapperProps) {
	const selectedModuleData = modules.find(([name]) => name === selectedModuleName);

	if (!selectedModuleData) {
		return null;
	}

	const [name, code] = selectedModuleData;

	return <ModuleView id={id} name={name} code={code} />;
}
const VALID_TABS = ['bytecode', 'code'];

function PkgModuleViewWrapper({ id, modules, splitPanelOrientation, initialTab }: Props) {
	const isMediumOrAbove = useBreakpoint('md');

	const modulenames = modules.map(([name]) => name);
	const [searchParams, setSearchParams] = useSearchParamsMerged();
	const [query, setQuery] = useState('');
	const [activeTab, setActiveTab] = useState(() =>
		initialTab && VALID_TABS.includes(initialTab) ? initialTab : 'bytecode',
	);
	const [packageFiles, setPackageFiles] = useState<PackageFile[]>([]);
	const [verificationResult, setVerificationResult] = useState<VerificationResult | null>(null);
	const [network] = useNetwork();

	useEffect(() => {
		if (!id) {
			return;
		}

		async function codeVerificationCheck() {
			const { status, data: fetchedVerificationCheckResult } = await axios.get<
				SuiVerificationCheckResultDto,
				AxiosResponse<SuiVerificationCheckResultDto>
			>(VerificationApiEndpoint.WELLDONE_STUDIO, {
				params: {
					network: network.toLowerCase(),
					packageId: id,
				},
			});

			if (status !== 200) {
				setVerificationResult({
					isVerified: false,
				});
				return;
			}
			console.log('fetchedVerificationCheckResult', fetchedVerificationCheckResult);

			if (fetchedVerificationCheckResult.errMsg) {
				setVerificationResult({
					isVerified: false,
				});

				return;
			}

			if (
				!(
					fetchedVerificationCheckResult.isVerified && fetchedVerificationCheckResult.verifiedSrcUrl
				)
			) {
				setVerificationResult({
					isVerified: false,
				});
				return;
			}

			const { status: VerifiedSrcResStatus, data: blob } = await axios.get<Blob>(
				fetchedVerificationCheckResult.verifiedSrcUrl,
				{
					responseType: 'blob',
				},
			);

			if (VerifiedSrcResStatus !== 200) {
				throw new Error('Network response was not ok');
			}

			new JSZip().loadAsync(blob).then((unzipped: JSZip) => {
				const filePromises: Promise<PackageFile>[] = [];
				unzipped.forEach((relativePath: string, file: JSZip.JSZipObject) => {
					if (!file.dir) {
						const filePromise = file.async('text').then(
							(content: string): PackageFile => ({
								relativePath: file.name,
								content: content,
							}),
						);
						filePromises.push(filePromise);
					}
				});

				Promise.all(filePromises).then((packageFiles) => {
					setPackageFiles(
						packageFiles.filter(
							(packageFile) =>
								!(
									packageFile.relativePath.includes('Move.toml') ||
									packageFile.relativePath.includes('Move.lock')
								),
						),
					);
					setVerificationResult({ ...fetchedVerificationCheckResult });
				});
			});
		}

		codeVerificationCheck().then();
	}, [id, network]);

	// Extract module in URL or default to first module in list
	const selectedModule =
		searchParams.get('module') && modulenames.includes(searchParams.get('module')!)
			? searchParams.get('module')!
			: modulenames[0];

	// If module in URL exists but is not in module list, then delete module from URL
	useEffect(() => {
		if (searchParams.has('module') && !modulenames.includes(searchParams.get('module')!)) {
			setSearchParams({}, { replace: true });
		}
	}, [searchParams, setSearchParams, modulenames]);

	const filteredModules =
		query === ''
			? modulenames
			: modules
					.filter(([name]) => name.toLowerCase().includes(query.toLowerCase()))
					.map(([name]) => name);

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

	const bytecodeContent = [
		{
			panel: (
				<div key="bytecode" className="h-full grow overflow-auto border-gray-45 pt-5 md:pl-7">
					<Tabs size="lg" value={activeTab} onValueChange={setActiveTab}>
						<TabsList>
							<TabsTrigger value="bytecode">Bytecode</TabsTrigger>
							<TabsTrigger value="code">
								Code {verificationResult?.isVerified ? <sup>✅</sup> : null}
							</TabsTrigger>
						</TabsList>
						<TabsContent value="bytecode">
							<div
								className={clsx(
									'overflow-auto',
									(splitPanelOrientation === 'horizontal' || !isMediumOrAbove) &&
										'h-verticalListLong',
								)}
							>
								<ModuleViewWrapper id={id} modules={modules} selectedModuleName={selectedModule} />
							</div>
						</TabsContent>
						<TabsContent value="code">
							<div
								className={clsx(
									'overflow-auto',
									(splitPanelOrientation === 'horizontal' || !isMediumOrAbove) &&
										'h-verticalListLong',
								)}
							>
								<VerifiedModuleViewWrapper
									id={id}
									modules={modules}
									packageFiles={packageFiles}
									setPackageFiles={setPackageFiles}
									verificationResult={verificationResult}
									setVerificationResult={setVerificationResult}
									selectedModuleName={selectedModule}
								/>
							</div>
						</TabsContent>
					</Tabs>
				</div>
			),
			defaultSize: 40,
		},
		{
			panel: (
				<div key="execute" className="h-full grow overflow-auto border-gray-45 pt-5 md:pl-7">
					<TabHeader size="md" title="Execute">
						<div
							className={clsx(
								'overflow-auto',
								(splitPanelOrientation === 'horizontal' || !isMediumOrAbove) &&
									'h-verticalListLong',
							)}
						>
							{id && selectedModule ? (
								<ModuleFunctionsInteraction
									// force recreating everything when we change modules
									key={`${id}-${selectedModule}`}
									packageId={id}
									moduleName={selectedModule}
								/>
							) : null}
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
						{modulenames.map((name) => (
							<div key={name} className="mx-0.5 mt-0.5 md:min-w-fit">
								<ListItem active={selectedModule === name} onClick={() => onChangeModule(name)}>
									{name}
								</ListItem>
							</div>
						))}
					</VerticalList>
				</div>
			</div>
			{isMediumOrAbove ? (
				<div className="w-4/5">
					<SplitPanes direction={splitPanelOrientation} splitPanels={bytecodeContent} />
				</div>
			) : (
				bytecodeContent.map((panel, index) => <div key={index}>{panel.panel}</div>)
			)}
		</div>
	);
}
export default PkgModuleViewWrapper;
