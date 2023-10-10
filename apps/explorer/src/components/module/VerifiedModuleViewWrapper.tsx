// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Button, LoadingIndicator, Text } from '@mysten/ui';
import axios, { type AxiosResponse } from 'axios';
import JSZip from 'jszip';
import { type Dispatch, type SetStateAction, useState } from 'react';

import ModuleView from '~/components/module/ModuleView';
import {
	type ModuleType,
	type PackageFile,
	type VerificationResult,
} from '~/components/module/PkgModulesWrapper';
import { type SuiNetwork } from '~/components/module/SuiNetwork';
import { VerificationApiEndpoint } from '~/components/module/VerificationApiEndpoint';
import VerifyRegister from '~/components/module/VerifyRegister';
import { type SuiVerificationCheckResultDto } from '~/components/module/dto/verification/SuiVerificationCheckResultDto';
import { type SuiVerificationReqDto } from '~/components/module/dto/verification/SuiVerificationReqDto';
import { type SuiVerificationResultDto } from '~/components/module/dto/verification/SuiVerificationResultDto';
import { type SuiVerificationSrcUploadReqDto } from '~/components/module/dto/verification/SuiVerificationSrcUploadReqDto';
import { type SuiVerificationSrcUploadResultDto } from '~/components/module/dto/verification/SuiVerificationSrcUploadResultDto';
import { useNetwork } from '~/context';

interface VerifiedModuleViewWrapperProps {
	id?: string;
	selectedModuleName: string;
	modules: ModuleType[];
	packageFiles: PackageFile[];
	setPackageFiles: Dispatch<SetStateAction<PackageFile[]>>;
	verificationResult: VerificationResult | null;
	setVerificationResult: Dispatch<SetStateAction<VerificationResult | null>>;
}

function VerifiedModuleViewWrapper({
	id,
	selectedModuleName,
	modules,
	packageFiles,
	setPackageFiles,
	verificationResult,
	setVerificationResult,
}: VerifiedModuleViewWrapperProps) {
	const [verificationApiServer, setVerificationApiServer] = useState<string>(
		VerificationApiEndpoint.WELLDONE_STUDIO,
	);
	const [network] = useNetwork();
	const [files, setFiles] = useState<File[]>([]);
	const [isLoading, setIsLoading] = useState<boolean>(false);
	const [errorMsg, setErrorMsg] = useState<string>();

	const selectedModuleData = modules.find(([name]) => name === selectedModuleName);
	if (!selectedModuleData) {
		return null;
	}
	const [name] = selectedModuleData;

	const regExpInput = `module\\s+\\w+::${name}\\s+{([\\s\\S]*?)^}`;
	const regExpFlag = `gm`;
	const reg = new RegExp(regExpInput, regExpFlag);
	const matchingModule = packageFiles.find((element: PackageFile) => reg.test(element.content));
	let code = '';
	if (matchingModule) {
		const reg = new RegExp(regExpInput, regExpFlag);
		const results = reg.exec(matchingModule.content);
		if (results?.length) {
			code = results[0];
		}
	}

	const onVerificationApiServerChange = (e: any) => {
		setVerificationApiServer(e.target.value);
	};

	const verify = async () => {
		if (!id) {
			return;
		}
		setIsLoading(true);
		setErrorMsg('');

		try {
			const { status, data: fetchedVerificationCheckResult } = await axios.get<
				SuiVerificationCheckResultDto,
				AxiosResponse<SuiVerificationCheckResultDto>
			>(verificationApiServer, {
				params: {
					network: network.toLowerCase(),
					packageId: id,
				},
			});

			if (status !== 200) {
				setVerificationResult({
					isVerified: false,
				});
				setErrorMsg(`Verification API Check Fetch Error. status=${status}`);
				return;
			}
			console.log('fetchedVerificationCheckResult', fetchedVerificationCheckResult);
			if (
				!(
					fetchedVerificationCheckResult.isVerified && fetchedVerificationCheckResult.verifiedSrcUrl
				)
			) {
				await verifyWithFile();
				return;
			}

			const { status: VerifiedSrcResStatus, data: blob } = await axios.get<Blob>(
				fetchedVerificationCheckResult.verifiedSrcUrl,
				{
					responseType: 'blob',
				},
			);

			if (VerifiedSrcResStatus !== 200) {
				setErrorMsg(`Verified Source Code Download Failed`);
				return;
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
					console.log('verified packageFiles', packageFiles);
					setPackageFiles(
						packageFiles.filter(
							(packageFile) =>
								!(
									packageFile.relativePath.includes('Move.toml') ||
									packageFile.relativePath.includes('Move.lock')
								),
						),
					);
					setVerificationResult({
						isVerified: fetchedVerificationCheckResult.isVerified,
					});
				});
			});
		} catch (e: any) {
			setErrorMsg(e.toString());
		} finally {
			setIsLoading(false);
		}
	};

	const verifyWithFile = async () => {
		if (!id) {
			return;
		}

		if (files.length === 0) {
			setErrorMsg(`Files Empty`);
			return;
		}
		setIsLoading(true);
		try {
			const { status: sourcesResStatus, data: sourcesResData } = await axios.post<
				SuiVerificationSrcUploadResultDto,
				AxiosResponse<SuiVerificationSrcUploadResultDto>,
				SuiVerificationSrcUploadReqDto
			>(
				`${verificationApiServer}/sources`,
				{
					network: network.toLowerCase() as SuiNetwork,
					packageId: id,
					srcZipFile: files[0],
				},
				{
					headers: {
						'Content-Type': 'multipart/form-data',
						Accept: 'application/json',
					},
				},
			);

			if (sourcesResStatus !== 201) {
				setErrorMsg(`Verification Source Upload API Error. status=${sourcesResStatus}`);
				return;
			}

			const { status: verificationResStatus, data: verificationResData } = await axios.post<
				SuiVerificationResultDto,
				AxiosResponse<SuiVerificationResultDto>,
				SuiVerificationReqDto
			>(`${verificationApiServer}`, {
				network: network.toLowerCase() as SuiNetwork,
				packageId: id,
				srcFileId: sourcesResData.srcFileId,
			});

			if (verificationResStatus !== 201) {
				setErrorMsg(`Verification request API error. status=${verificationResStatus}`);
				return;
			}

			if (verificationResData.errMsg) {
				setErrorMsg(verificationResData.errMsg);
				return;
			}

			if (!verificationResData.verifiedSrcUrl) {
				setErrorMsg(`Verified Source URL Failed.`);
				return;
			}

			const { status: VerifiedSrcResStatus, data: blob } = await axios.get<Blob>(
				verificationResData.verifiedSrcUrl,
				{
					responseType: 'blob',
				},
			);

			if (VerifiedSrcResStatus !== 200) {
				setErrorMsg(`Verified source download Failed.`);
				return;
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
					console.log('verified packageFiles', packageFiles);
					setPackageFiles(
						packageFiles.filter(
							(packageFile) =>
								!(
									packageFile.relativePath.includes('Move.toml') ||
									packageFile.relativePath.includes('Move.lock')
								),
						),
					);
					setVerificationResult({
						isVerified: verificationResData.isVerified,
					});
				});
			});

			setVerificationResult({
				isVerified: false,
			});
			setErrorMsg(verificationResData.errMsg || '');
		} catch (e: any) {
			console.error(e);
			setErrorMsg(e.toString());
		} finally {
			setIsLoading(false);
		}
	};

	if (!verificationResult) {
		return (
			<div className="flex h-full items-center justify-center">
				<LoadingIndicator />
				<Text color="steel" variant="body/medium">
					loading data
				</Text>
			</div>
		);
	}

	return (
		<>
			<div className="flex flex-wrap items-center justify-items-center gap-2">
				<div className="flex flex-wrap items-center justify-items-center gap-2">
					<input
						onChange={onVerificationApiServerChange}
						value={verificationApiServer}
						style={{
							width: '32em',
							border: '1px solid #ccc',
							borderRadius: '0.3em',
							padding: '0.1em 0.3em',
						}}
					/>
				</div>
				<Button type="submit" variant="primary" size="md" loading={isLoading} onClick={verify}>
					Verify
				</Button>
			</div>
			<div className="mt-3">
				<Text variant="body/medium" color="issue">
					{errorMsg}
				</Text>
			</div>

			{!errorMsg && verificationResult?.isVerified ? (
				<ModuleView id={id} name={name} code={code} />
			) : (
				<VerifyRegister modules={modules} files={files} setFiles={setFiles} />
			)}
		</>
	);
}

export default VerifiedModuleViewWrapper;
