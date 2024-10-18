// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { readFile, writeFile } from 'node:fs/promises';
import { deserialize } from '@mysten/move-bytecode-template';
import { normalizeSuiAddress } from '@mysten/sui/utils';
import ts from 'typescript';

import type { DeserializedModule, TypeSignature } from './types.js';
import { mapToObject, parseTS, printStatements } from './utils.js';

const MOVE_STDLIB_ADDRESS = normalizeSuiAddress('0x1').slice(2);
const SUI_FRAMEWORK_ADDRESS = normalizeSuiAddress('0x2').slice(2);

class ModuleBuilder {
	statements: ts.Statement[] = [];
	moduleDef: DeserializedModule;
	exports: string[] = [];
	usesGenerics = false;

	constructor(moduleDef: DeserializedModule) {
		this.moduleDef = moduleDef;
	}

	static async fromFile(file: string) {
		return new ModuleBuilder(deserialize(await readFile(file)));
	}

	renderBCSTypes() {
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
				this.resolveTypeSignature(field.signature),
			]);

			const params = handle.type_parameters.filter((param) => !param.is_phantom);

			if (params.length === 0) {
				this.statements.push(
					...parseTS/* ts */ `function ${name}() {
						return bcs.struct('${name}', ${fieldObject})
					}`,
				);
			} else {
				this.usesGenerics = true;

				const typeParams = `...typeParameters: [${params.map((_, i) => `T${i}`).join(', ')}]`;
				const typeGenerics = `${params.map((_, i) => `T${i} extends BcsType<any>`).join(', ')}`;

				this.statements.push(
					...parseTS/* ts */ `function ${name}<${typeGenerics}>(${typeParams}) {
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
					signature: this.resolveTypeSignature(field.signature),
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
					function ${name}( ) {
						return bcs.enum('${name}', ${variantsObject})
					}`,
				);
			} else {
				this.usesGenerics = true;

				const typeParams = `...typeParameters: [${params.map((_, i) => `T${i}`).join(', ')}]`;
				const typeGenerics = `${params.map((_, i) => `T${i} extends BcsType<any>`).join(', ')}`;

				this.statements.push(
					...parseTS/* ts */ `
					function ${name}<${typeGenerics}>(${typeParams}) {
						return bcs.enum('${name}', ${variantsObject})
					}`,
				);
			}
		}
	}

	resolveTypeSignature(type: TypeSignature): string {
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
			return this.resolveDatatypeReference(type.Datatype);
		}

		if ('DatatypeInstantiation' in type) {
			const [datatype, typeParameters] = type.DatatypeInstantiation;
			return this.resolveDatatypeReference(datatype, typeParameters);
		}

		if ('Vector' in type) {
			return `bcs.vector(${this.resolveTypeSignature(type.Vector)})`;
		}

		if ('TypeParameter' in type) {
			return `typeParameters[${type.TypeParameter}]`;
		}

		throw new Error(`Unknown type signature: ${JSON.stringify(type, null, 2)}`);
	}

	resolveDatatypeReference(type: number, typeParameters: TypeSignature[] = []): string {
		const handle = this.moduleDef.datatype_handles[type];
		const typeName = this.moduleDef.identifiers[handle.name];

		if (handle.module === this.moduleDef.self_module_handle_idx) {
			return `${typeName}(${typeParameters.map((type) => this.resolveTypeSignature(type)).join(', ')})`;
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
				return `bcs.option(${this.resolveTypeSignature(typeParameters[0])})`;
			}
		}

		if (moduleAddress === SUI_FRAMEWORK_ADDRESS) {
			if (moduleName === 'object' && typeName === 'ID') {
				return `bcs.Address`;
			}
		}

		return `${moduleName}.${typeName}(
			${typeParameters.map((type) => this.resolveTypeSignature(type)).join(', ')}`;
	}

	toString() {
		const bcsImports = ['bcs'];
		if (this.usesGenerics) {
			bcsImports.push('type BcsType');
		}

		return printStatements(
			parseTS/* ts */ `import { ${bcsImports.join(', ')} } from '@mysten/sui/bcs';

			export function ${'mod'}() {
				${this.statements}

				return ${ts.factory.createObjectLiteralExpression(
					this.exports.map((name) =>
						ts.factory.createShorthandPropertyAssignment(ts.factory.createIdentifier(name)),
					),
					true,
				)}
			}
			`,
		);
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
		const module = builder.moduleDef.module_handles[builder.moduleDef.self_module_handle_idx];
		await writeFile(
			`./tests/generated/${builder.moduleDef.identifiers[module.name]}.ts`,
			builder.toString(),
		);
	}
}

main().then(console.log, console.error);
