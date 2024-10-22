// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { readFile, writeFile } from 'node:fs/promises';
import { deserialize } from '@mysten/move-bytecode-template';
import { normalizeSuiAddress } from '@mysten/sui/utils';
import type ts from 'typescript';

import type { DeserializedModule, TypeSignature } from './types.js';
import { mapToObject, parseTS, printStatements } from './utils.js';

const MOVE_STDLIB_ADDRESS = normalizeSuiAddress('0x1').slice(2);
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
		return new ModuleBuilder(deserialize(await readFile(file)));
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
				this.bcsFromTypeSignature(field.signature),
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
					signature: this.bcsFromTypeSignature(field.signature),
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

			names.push(name);
			statements.push(
				...parseTS/* ts */ `function ${name}(options: {
					arguments: [
					${this.moduleDef.signatures[handle.parameters]
						.filter((param) => !this.isContextReference(param))
						.map((param) => this.typeFromTypeSignature(param))
						.join(',\n')}],
				}) {
					return (tx: Transaction) => tx.moveCall({
						package: packageAddress,
						module: '${moduleName}',
						function: '${name}',
						arguments: options.arguments, // TODO: map arguments
						${false ? 'typeArguments: options.typeArguments' : ''}
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

	bcsFromTypeSignature(type: TypeSignature): string {
		switch (type) {
			case 'Address':
				return `bcs.Address`;
			case 'Bool':
				return `bcs.bool()`;
			case 'U8':
				return `bcs.u8()`;
			case 'U16':
				return `bcs.u16()`;
			case 'U32':
				return `bcs.u32()`;
			case 'U64':
				return `bcs.u64()`;
			case 'U128':
				return `bcs.u128()`;
			case 'U256':
				return `bcs.u256()`;
		}

		if ('Datatype' in type) {
			return this.bcsFromDatatypeReference(type.Datatype);
		}

		if ('DatatypeInstantiation' in type) {
			const [datatype, typeParameters] = type.DatatypeInstantiation;
			return this.bcsFromDatatypeReference(datatype, typeParameters);
		}

		if ('Reference' in type) {
			return this.bcsFromTypeSignature(type.Reference);
		}

		if ('MutableReference' in type) {
			return this.bcsFromTypeSignature(type.MutableReference);
		}

		if ('Vector' in type) {
			return `bcs.vector(${this.bcsFromTypeSignature(type.Vector)})`;
		}

		if ('TypeParameter' in type) {
			return `typeParameters[${type.TypeParameter}]`;
		}

		throw new Error(`Unknown type signature: ${JSON.stringify(type, null, 2)}`);
	}

	typeFromTypeSignature(type: TypeSignature): string {
		switch (type) {
			case 'Address':
				return `string`;
			case 'Bool':
				return `boolean`;
			case 'U8':
			case 'U16':
			case 'U32':
			case 'U64':
			case 'U128':
			case 'U256':
				return 'number';
		}

		if ('Datatype' in type) {
			return this.typeFromDatatypeReference(type.Datatype);
		}

		if ('DatatypeInstantiation' in type) {
			const [datatype, typeParameters] = type.DatatypeInstantiation;
			return this.typeFromDatatypeReference(datatype, typeParameters);
		}

		if ('Reference' in type) {
			return this.typeFromTypeSignature(type.Reference);
		}

		if ('MutableReference' in type) {
			return this.typeFromTypeSignature(type.MutableReference);
		}

		if ('Vector' in type) {
			return `${this.typeFromTypeSignature(type.Vector)}[]`;
		}

		if ('TypeParameter' in type) {
			return `T${type.TypeParameter}`;
		}

		throw new Error(`Unknown type signature: ${JSON.stringify(type, null, 2)}`);
	}

	bcsFromDatatypeReference(type: number, typeParameters: TypeSignature[] = []): string {
		const handle = this.moduleDef.datatype_handles[type];
		const typeName = this.moduleDef.identifiers[handle.name];

		if (handle.module === this.moduleDef.self_module_handle_idx) {
			return `${typeName}(${typeParameters.map((type) => this.bcsFromTypeSignature(type)).join(', ')})`;
		}

		const moduleHandle = this.moduleDef.module_handles[handle.module];
		const moduleAddress = this.moduleDef.address_identifiers[moduleHandle.address];
		const moduleName = this.moduleDef.identifiers[moduleHandle.name];

		if (moduleAddress === MOVE_STDLIB_ADDRESS) {
			if (moduleName === 'ascii' && typeName === 'String') {
				return `bcs.string()`;
			}
			if (moduleName === 'string' && typeName === 'String') {
				return `bcs.string()`;
			}

			if (moduleName === 'option' && typeName === 'Option') {
				return `bcs.option(${this.bcsFromTypeSignature(typeParameters[0])})`;
			}
		}

		if (moduleAddress === SUI_FRAMEWORK_ADDRESS) {
			if (moduleName === 'object' && typeName === 'ID') {
				return `bcs.Address`;
			}
		}

		this.addStarImport(`./deps/${moduleAddress}/${moduleName}`, moduleName);
		return `${moduleName}.${typeName}(
			${typeParameters.map((type) => this.bcsFromTypeSignature(type)).join(', ')})`;
	}

	typeFromDatatypeReference(type: number, typeParameters: TypeSignature[] = []): string {
		const handle = this.moduleDef.datatype_handles[type];
		const typeName = this.moduleDef.identifiers[handle.name];

		const moduleHandle = this.moduleDef.module_handles[handle.module];
		const moduleAddress = this.moduleDef.address_identifiers[moduleHandle.address];
		const moduleName = this.moduleDef.identifiers[moduleHandle.name];

		if (moduleAddress === MOVE_STDLIB_ADDRESS) {
			if (moduleName === 'ascii' && typeName === 'String') {
				return `string`;
			}
			if (moduleName === 'string' && typeName === 'String') {
				return `string`;
			}

			if (moduleName === 'option' && typeName === 'Option') {
				// TODO: handle option of struct
				return `${this.typeFromTypeSignature(typeParameters[0])} | null`;
			}
		}

		if (moduleAddress === SUI_FRAMEWORK_ADDRESS) {
			if (moduleName === 'object' && typeName === 'ID') {
				return `string`;
			}
		}

		const typeNameRef =
			handle.module === this.moduleDef.self_module_handle_idx
				? typeName
				: `${moduleName}.${typeName}`;

		if (handle.module !== this.moduleDef.self_module_handle_idx) {
			this.addStarImport(`./deps/${moduleAddress}/${moduleName}`, moduleName);
		}

		if (typeParameters.length === 0) {
			return `ReturnType<typeof ${typeNameRef}>['$inferType']`;
		}

		return `ReturnType<typeof ${typeNameRef}<${typeParameters.map((type) => this.typeFromTypeSignature(type)).join(', ')}>>['$inferType']`;
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

	toString() {
		const importStatements = [...this.imports.entries()].flatMap(
			([module, names]) => parseTS`import { ${[...names].join(', ')} } from '${module}'`,
		);
		const starImportStatements = [...this.starImports.entries()].flatMap(
			([name, module]) => parseTS`import * as ${name} from '${module}'`,
		);

		return printStatements([...importStatements, ...starImportStatements, ...this.statements]);
	}
}

async function main() {
	const modules = [
		'./tests/move/paywalrus/build/paywalrus/bytecode_modules/feed.mv',
		'./tests/move/paywalrus/build/paywalrus/bytecode_modules/policy.mv',
		'./tests/move/paywalrus/build/paywalrus/bytecode_modules/other.mv',
	];

	const builders = await Promise.all(modules.map(ModuleBuilder.fromFile));

	for (const builder of builders) {
		builder.renderBCSTypes();
		builder.renderFunctions();
		const module = builder.moduleDef.module_handles[builder.moduleDef.self_module_handle_idx];
		await writeFile(
			`./tests/generated/${builder.moduleDef.identifiers[module.name]}.ts`,
			builder.toString(),
		);
	}
}

main().then(console.log, console.error);
