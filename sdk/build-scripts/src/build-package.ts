#! /usr/bin/env tsx
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { promises as fs, existsSync } from 'fs';
import * as path from 'path';
import { build } from 'esbuild';
import { execSync } from 'child_process';

interface PackageJSON {
	name: string;
	exports?: Record<string, string | Record<string, string>>;
	files?: string[];
}

const ignorePatterns = [/\.test.ts$/];

buildPackage().catch((error) => {
	console.error(error);
	process.exit(1);
});

async function buildPackage() {
	const allFiles = await findAllFiles(path.join(process.cwd(), 'src'));
	const packageJson = await readPackageJson();
	await clean();
	await buildCJS(allFiles);
	await buildESM(allFiles);
	await buildImportDirectories(packageJson);
}

async function findAllFiles(dir: string, files: string[] = []) {
	const dirFiles = await fs.readdir(dir);
	for (const file of dirFiles) {
		const filePath = path.join(dir, file);
		const fileStat = await fs.stat(filePath);
		if (fileStat.isDirectory()) {
			await findAllFiles(filePath, files);
		} else if (!ignorePatterns.some((pattern) => pattern.test(filePath))) {
			files.push(filePath);
		}
	}
	return files;
}

async function clean() {
	await createEmptyDir(path.join(process.cwd(), 'dist'));
}

async function buildCJS(entryPoints: string[]) {
	await build({
		format: 'cjs',
		logLevel: 'error',
		target: 'es2020',
		entryPoints,
		outdir: 'dist/cjs',
		sourcemap: true,
	});
	await buildTypes('tsconfig.json');
	await fs.writeFile(
		path.join(process.cwd(), 'dist/package.json'),
		JSON.stringify(
			{
				private: true,
				type: 'commonjs',
			},
			null,
			2,
		),
	);
}

async function buildESM(entryPoints: string[]) {
	await build({
		format: 'esm',
		logLevel: 'error',
		target: 'es2020',
		entryPoints,
		outdir: 'dist/esm',
		sourcemap: true,
	});
	await buildTypes('tsconfig.esm.json');
	await fs.writeFile(
		path.join(process.cwd(), 'dist/esm/package.json'),
		JSON.stringify(
			{
				private: true,
				type: 'module',
			},
			null,
			2,
		),
	);
}

async function buildTypes(config: string) {
	execSync(`pnpm tsc --build ${config}`, {
		stdio: 'inherit',
		cwd: process.cwd(),
	});
}

async function buildImportDirectories({ exports }: PackageJSON) {
	if (!exports) {
		return;
	}

	const exportDirs = new Set<string>();
	const ignoredWorkspaces = [];

	for (const [exportName, exportMap] of Object.entries(exports)) {
		if (typeof exportMap !== 'object' || !exportName.match(/^\.\/[\w\-_/]+$/)) {
			continue;
		}

		const exportDir = path.join(process.cwd(), exportName);
		const parts = exportName.split('/');
		exportDirs.add(parts[1]);

		if (parts.length === 2) {
			ignoredWorkspaces.push(path.relative(path.resolve(process.cwd(), '../..'), exportDir));
		}

		await createEmptyDir(exportDir);
		await fs.writeFile(
			path.join(exportDir, 'package.json'),
			`${JSON.stringify(
				{
					private: true,
					types:
						exportMap.types &&
						path.relative(exportDir, path.resolve(process.cwd(), exportMap.types)),
					import:
						exportMap.import &&
						path.relative(exportDir, path.resolve(process.cwd(), exportMap.import)),
					main: path.relative(
						exportDir,
						path.resolve(process.cwd(), exportMap.require ?? exportMap.default),
					),
				},
				null,
				'\t',
			)}\n`,
		);
	}

	await addPackageFiles([...exportDirs]);
	await addIgnoredWorkspaces(ignoredWorkspaces);
}

async function createEmptyDir(path: string) {
	if (existsSync(path)) {
		await fs.rm(path, { recursive: true });
	}

	await fs.mkdir(path, { recursive: true });
}

async function readPackageJson() {
	return JSON.parse(
		await fs.readFile(path.join(process.cwd(), 'package.json'), 'utf-8'),
	) as PackageJSON;
}

async function addPackageFiles(paths: string[]) {
	const json = await readPackageJson();

	if (!json.files) {
		return;
	}

	for (const path of paths) {
		if (!json.files.includes(path)) {
			json.files.push(path);
		}
	}

	json.files.sort();

	await fs.writeFile(
		path.join(process.cwd(), 'package.json'),
		`${JSON.stringify(json, null, '\t')}\n`,
	);
}

async function addIgnoredWorkspaces(paths: string[]) {
	const file = await fs.readFile(path.join(process.cwd(), '../../pnpm-workspace.yaml'), 'utf-8');
	const lines = file.split('\n').filter(Boolean);
	let changed = false;

	for (const path of paths) {
		if (!lines.find((line) => line.includes(`!${path}`))) {
			changed = true;
			lines.push(`    - '!${path}'`);
		}
	}

	if (changed) {
		lines.push('');
		await fs.writeFile(path.join(process.cwd(), '../../pnpm-workspace.yaml'), lines.join('\n'));
	}
}
