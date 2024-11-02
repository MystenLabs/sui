// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { mkdir, readdir, readFile, writeFile } from 'node:fs/promises';
import { relative } from 'node:path';
import { deserialize } from '@mysten/move-bytecode-template';
import { normalizeSuiAddress } from '@mysten/sui/utils';
import type ts from 'typescript';

import { renderTypeSignature } from './render-types.js';
import type { DeserializedModule, TypeSignature } from './types.js';
import { mapToObject, parseTS, printStatements } from './utils.js';

const SUI_FRAMEWORK_ADDRESS = normalizeSuiAddress('0x2').slice(2);

class ModuleBuilder {
	statements: ts.Statement[] = [];
	moduleDef: DeserializedModule;
	exports: string[] = [];
	imports: Map<string, Set<string>> = new Map();
	starImports: Map<string, string> = new Map();

	constructor(moduleDef: DeserializedModule) {
		this.moduleDef = moduleDef;
	}

	static async fromFile(file: string) {
		const bytes = await readFile(file);

		return new ModuleBuilder(deserialize(bytes));
	}

	addImport(module: string, name: string) {
		if (!this.imports.has(module)) {
			this.imports.set(module, new Set());
		}

		this.imports.get(module)!.add(name);
	}

	addStarImport(module: string, name: string) {
		this.starImports.set(name, module);
	}

	renderBCSTypes() {
		this.addImport('@mysten/sui/bcs', 'bcs');
		this.renderStructs();
		this.renderEnums();
	}

	renderStructs() {
		for (const struct of this.moduleDef.struct_defs) {
			const handle = this.moduleDef.datatype_handles[struct.struct_handle];
			const name = this.moduleDef.identifiers[handle.name];
			this.exports.push(name);

			const fields =
				struct.field_information.Declared?.map((field) => ({
					name: this.moduleDef.identifiers[field.name],
					signature: field.signature,
				})) ?? [];

			const fieldObject = mapToObject(fields, (field) => [
				field.name,
				renderTypeSignature(field.signature, {
					format: 'bcs',
					moduleDef: this.moduleDef,
					onDependency: (address, mod) => this.addStarImport(`./deps/${address}/${mod}`, mod),
				}),
			]);

			const params = handle.type_parameters.filter((param) => !param.is_phantom);

			if (params.length === 0) {
				this.statements.push(
					...parseTS/* ts */ `export function ${name}() {
						return bcs.struct('${name}', ${fieldObject})
					}`,
				);
			} else {
				this.addImport('@mysten/sui/bcs', 'type BcsType');

				const typeParams = `...typeParameters: [${params.map((_, i) => `T${i}`).join(', ')}]`;
				const typeGenerics = `${params.map((_, i) => `T${i} extends BcsType<any>`).join(', ')}`;

				this.statements.push(
					...parseTS/* ts */ `export function ${name}<${typeGenerics}>(${typeParams}) {
						return bcs.struct('${name}', ${fieldObject})
					}`,
				);
			}
		}
	}

	renderEnums() {
		for (const enumDef of this.moduleDef.enum_defs) {
			const handle = this.moduleDef.datatype_handles[enumDef.enum_handle];
			const name = this.moduleDef.identifiers[handle.name];
			this.exports.push(name);

			const variants = enumDef.variants.map((variant) => ({
				name: this.moduleDef.identifiers[variant.variant_name],
				fields: variant.fields.map((field) => ({
					name: this.moduleDef.identifiers[field.name],
					signature: renderTypeSignature(field.signature, {
						format: 'bcs',
						moduleDef: this.moduleDef,
						onDependency: (address, mod) => this.addStarImport(`./deps/${address}/${mod}`, mod),
					}),
				})),
			}));

			const variantsObject = mapToObject(variants, (variant) => [
				variant.name,
				variant.fields.length === 0
					? 'null'
					: variant.fields.length === 1
						? variant.fields[0].signature
						: `bcs.tuple([${variant.fields.map((field) => field.signature).join(', ')}])`,
			]);

			const params = handle.type_parameters.filter((param) => !param.is_phantom);

			if (params.length === 0) {
				this.statements.push(
					...parseTS/* ts */ `
					export function ${name}( ) {
						return bcs.enum('${name}', ${variantsObject})
					}`,
				);
			} else {
				this.addImport('@mysten/sui/bcs', 'type BcsType');

				const typeParams = `...typeParameters: [${params.map((_, i) => `T${i}`).join(', ')}]`;
				const typeGenerics = `${params.map((_, i) => `T${i} extends BcsType<any>`).join(', ')}`;

				this.statements.push(
					...parseTS/* ts */ `
					export function ${name}<${typeGenerics}>(${typeParams}) {
						return bcs.enum('${name}', ${variantsObject})
					}`,
				);
			}
		}
	}

	renderFunctions() {
		const statements = [];
		const names = [];

		if (this.moduleDef.function_defs.length !== 0) {
			this.addImport('@mysten/sui/transactions', 'type Transaction');
		}

		for (const func of this.moduleDef.function_defs) {
			const handle = this.moduleDef.function_handles[func.function];
			const name = this.moduleDef.identifiers[handle.name];
			const moduleName =
				this.moduleDef.identifiers[this.moduleDef.module_handles[handle.module].name];
			const parameters = this.moduleDef.signatures[handle.parameters].filter(
				(param) => !this.isContextReference(param),
			);

			this.addImport('./utils/index.ts', 'normalizeMoveArguments');
			this.addImport('./utils/index.ts', 'type RawTransactionArgument');

			names.push(name);
			statements.push(
				...parseTS/* ts */ `function
					${name}${
						handle.type_parameters.length
							? `<
							${handle.type_parameters.map((_, i) => `T${i} extends BcsType<any>`)}
						>`
							: ''
					}(options: {
						arguments: [
						${parameters
							.map((param) =>
								renderTypeSignature(param, {
									format: 'typescriptArg',
									moduleDef: this.moduleDef,
									onDependency: (address, mod) =>
										this.addStarImport(`./deps/${address}/${mod}`, mod),
								}),
							)
							.map((type) => `RawTransactionArgument<${type}>`)
							.join(',\n')}],

						${
							handle.type_parameters.length
								? `typeArguments: [${handle.type_parameters.map(() => 'string').join(', ')}]`
								: ''
						}
				}) {
					const argumentsTypes = [
						${parameters
							.map((param) =>
								renderTypeSignature(param, { format: 'typeTag', moduleDef: this.moduleDef }),
							)
							.map((tag) => (tag.includes('{') ? `\`${tag}\`` : `'${tag}'`))
							.join(',\n')}
					]
					return (tx: Transaction) => tx.moveCall({
						package: packageAddress,
						module: '${moduleName}',
						function: '${name}',
						arguments: normalizeMoveArguments(options.arguments, argumentsTypes),
						${handle.type_parameters.length ? 'typeArguments: options.typeArguments' : ''}
					})
				}`,
			);
		}

		this.statements.push(
			...parseTS/* ts */ `
			export function init(packageAddress: string) {
				${statements}

				return { ${names.join(', ')} }
			}`,
		);
	}

	isContextReference(type: TypeSignature): boolean {
		if (typeof type === 'string') {
			return false;
		}

		if ('Reference' in type) {
			return this.isContextReference(type.Reference);
		}

		if ('MutableReference' in type) {
			return this.isContextReference(type.MutableReference);
		}

		if ('Datatype' in type) {
			const handle = this.moduleDef.datatype_handles[type.Datatype];
			const moduleHandle = this.moduleDef.module_handles[handle.module];
			const address = this.moduleDef.address_identifiers[moduleHandle.address];
			const name = this.moduleDef.identifiers[handle.name];

			return address === SUI_FRAMEWORK_ADDRESS && name === 'TxContext';
		}

		return false;
	}

	toString(path: string) {
		const importStatements = [...this.imports.entries()].flatMap(
			([module, names]) =>
				parseTS`import { ${[...names].join(', ')} } from '${modulePath(module)}'`,
		);
		const starImportStatements = [...this.starImports.entries()].flatMap(
			([name, module]) => parseTS`import * as ${name} from '${modulePath(module)}'`,
		);

		return printStatements([...importStatements, ...starImportStatements, ...this.statements]);

		function modulePath(mod: string) {
			if (mod.startsWith('./')) {
				return relative(path, mod).slice(1);
			}

			return mod;
		}
	}
}

async function main() {
	const modules = [
		'./tests/move/paywalrus/build/paywalrus/bytecode_modules/feed.mv',
		'./tests/move/paywalrus/build/paywalrus/bytecode_modules/policy.mv',
		'./tests/move/paywalrus/build/paywalrus/bytecode_modules/other.mv',
		'./tests/move/paywalrus/build/paywalrus/bytecode_modules/managed.mv',
	];

	const builders = await Promise.all(modules.map(ModuleBuilder.fromFile));

	for (const builder of builders) {
		builder.renderBCSTypes();
		builder.renderFunctions();
		const module = builder.moduleDef.module_handles[builder.moduleDef.self_module_handle_idx];
		await writeFile(
			`./tests/generated/${builder.moduleDef.identifiers[module.name]}.ts`,
			builder.toString(`./${builder.moduleDef.identifiers[module.name]}.ts`),
		);

		console.log(builder.moduleDef.function_defs[2].code);
	}

	const depDirs = await readdir(
		'./tests/move/paywalrus/build/paywalrus/bytecode_modules/dependencies',
	);

	for (const dir of depDirs) {
		const modules = await readdir(
			`./tests/move/paywalrus/build/paywalrus/bytecode_modules/dependencies/${dir}`,
		);

		for (const modFile of modules) {
			let builder;
			try {
				builder = await ModuleBuilder.fromFile(
					`./tests/move/paywalrus/build/paywalrus/bytecode_modules/dependencies/${dir}/${modFile}`,
				);
			} catch (e) {
				console.log(e);
				continue;
			}
			const module = builder.moduleDef.module_handles[builder.moduleDef.self_module_handle_idx];
			const moduleName = builder.moduleDef.identifiers[module.name];
			const moduleAddress = builder.moduleDef.address_identifiers[module.address];
			builder.renderBCSTypes();
			await mkdir(`./tests/generated/deps/${moduleAddress}`, { recursive: true });
			await writeFile(
				`./tests/generated/deps/${moduleAddress}/${moduleName}.ts`,
				builder.toString(`./deps/${moduleAddress}/${moduleName}.ts`),
			);
		}
	}
}

main().then(console.log, console.error);
