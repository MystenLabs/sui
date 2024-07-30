// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as fs from 'fs';
import * as path from 'path';
import * as toml from '@iarna/toml';

type MoveToml = {
	package: Record<string, any>;
	dependencies: Record<string, any>;
	addresses: Record<string, any>;
};

export const parseMoveToml = (tomlPath: string) => {
	// Read the TOML file
	const tomlStr = fs.readFileSync(tomlPath, 'utf8');
	// Parse the TOML file
	const parsedToml = toml.parse(tomlStr);
	return parsedToml as MoveToml;
};

export const writeMoveToml = (tomlContent: MoveToml, outPath: string) => {
	let tomlFileContent = toml.stringify(tomlContent);
	fs.writeFileSync(outPath, tomlFileContent);
};

export const writeToml = (pkgPath: string, packageId: string, addressName: string) => {
	const tomlPath = path.join(pkgPath, 'Move.toml');
	const moveToml = parseMoveToml(tomlPath);
	moveToml.package['published-at'] = packageId;
	const addresses = moveToml.addresses;
	addresses[addressName] = packageId;
	const newTomlPath = path.join(pkgPath, `Move.toml`);
	writeMoveToml(moveToml, newTomlPath);
};
