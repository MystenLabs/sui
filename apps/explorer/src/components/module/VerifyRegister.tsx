// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Text } from '@mysten/ui';
import { type Dispatch, useState } from 'react';
import FileUpload from 'react-material-file-upload';

import { type ModuleType } from '~/components/module/PkgModulesWrapper';

interface VerifyRegisterProps {
	modules?: ModuleType[];
	files: File[];
	setFiles: Dispatch<File[]>;
}

function VerifyRegister({ modules, files, setFiles }: VerifyRegisterProps) {
	const modulenames = modules?.map(([name]) => name);
	const [query] = useState('');
	if (!modulenames) {
		return null;
	}

	const filteredModules =
		query === ''
			? modulenames
			: modules
					?.filter(([name]) => name.toLowerCase().includes(query.toLowerCase()))
					.map(([name]) => name);
	if (!filteredModules) {
		return null;
	}
	const onFileChange = (files: File[]) => {
		setFiles(files);
	};

	return (
		<div className="flex flex-col gap-1 border-b border-gray-45 md:flex md:flex-nowrap">
			<div className="mb-1 mt-5">
				<Text variant="body/medium" color="steel-dark">
					Otherwise You can proceed verification with uploading a compressed file.
				</Text>
			</div>
			<FileUpload value={files} maxFiles={1} onChange={onFileChange} />
			<div className="mb-1 ml-1 mt-2">
				<Text variant="body/medium" color="gray-100">
					1. Run this command &ldquo;zip -r your_source.zip .&rdquo; at the same directory path of
					&ldquo;Move.toml&rdquo;.
				</Text>
			</div>
			<div className="mb-2 ml-1">
				<Text variant="body/medium" color="gray-100">
					2. Drag the zip file above upload box.
				</Text>
			</div>
			<div className="mb-2 ml-1">
				<Text variant="body/medium" color="issue">
					* All dependencies in Move.toml should be git.
				</Text>
			</div>
		</div>
	);
}

export default VerifyRegister;
