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

	for (const [exportName, exportMap] of Object.entries(exports)) {
		if (typeof exportMap !== 'object' || !exportName.match(/^\.\/[\w\-_/]+$/)) {
			continue;
		}

		const exportDir = path.join(process.cwd(), exportName);
		await createEmptyDir(exportDir);
		await fs.writeFile(
			path.join(exportDir, 'package.json'),
			`${JSON.stringify(
				{
					private: true,
					types: exportMap.types?.replace(/^\.\//, '../'),
					import: exportMap.import?.replace(/^\.\//, '../'),
					main: (exportMap.require ?? exportMap.default)?.replace(/^\.\//, '../'),
				},
				null,
				2,
			).replace(/^ {2}/gm, '\t')}\n`,
		);
	}
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
