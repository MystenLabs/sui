import { bcs, BcsType, TypeTag, TypeTagSerializer } from '@mysten/sui/bcs';
import { normalizeSuiAddress } from '@mysten/sui/utils';
import { TransactionArgument, isArgument } from '@mysten/sui/transactions';

const MOVE_STDLIB_ADDRESS = normalizeSuiAddress('0x1');
const SUI_FRAMEWORK_ADDRESS = normalizeSuiAddress('0x2');

export type RawTransactionArgument<T> = T | TransactionArgument

export function getPureBcsSchema(typeTag: string | TypeTag): BcsType<any> | null {
	const parsedTag = typeof typeTag === 'string' ? TypeTagSerializer.parseFromStr(typeTag) : typeTag;

	if ('u8' in parsedTag) {
		return bcs.U8;
	} else if ('u16' in parsedTag) {
		return bcs.U16;
	} else if ('u32' in parsedTag) {
		return bcs.U32;
	} else if ('u64' in parsedTag) {
		return bcs.U64;
	} else if ('u128' in parsedTag) {
		return bcs.U128;
	} else if ('u256' in parsedTag) {
		return bcs.U256;
	} else if ('address' in parsedTag) {
		return bcs.Address;
	} else if ('bool' in parsedTag) {
		return bcs.Bool;
	} else if ('vector' in parsedTag) {
		const type = getPureBcsSchema(parsedTag.vector);
		return type ? bcs.vector(type) : null;
	} else if ('struct' in parsedTag) {
		const structTag = parsedTag.struct;
		const pkg = normalizeSuiAddress(parsedTag.struct.address);

		if (pkg === MOVE_STDLIB_ADDRESS) {
			if (
				(structTag.module === 'ascii' || structTag.module === 'string') &&
				structTag.name === 'String'
			) {
				return bcs.String;
			}

			if (structTag.module === 'option' && structTag.name === 'Option') {
				const type = getPureBcsSchema(structTag.typeParams[0]);
				return type ? bcs.vector(type) : null;
			}
		}

		if (pkg === SUI_FRAMEWORK_ADDRESS && structTag.module === 'Object' && structTag.name === 'ID') {
			return bcs.Address;
		}
	}

	return null;
}

export function normalizeMoveArguments(args: unknown[], argTypes: string[]) {
	if (args.length !== argTypes.length) {
		throw new Error(`Invalid number of arguments, expected ${argTypes.length}, got ${args.length}`);
	}

	const normalizedArgs: TransactionArgument[] = [];

	for (const [i, arg] of args.entries()) {
		if (typeof arg === 'function' || isArgument(arg)) {
			normalizedArgs.push(arg as TransactionArgument);
			continue;
		}

		const type = argTypes[i];
		const bcsType = getPureBcsSchema(type);

		if (bcsType) {
			const bytes = bcsType.serialize(arg as never);
			normalizedArgs.push(tx => tx.pure(bytes));
			continue;
		} else if (typeof arg === 'string') {
			normalizedArgs.push(tx => tx.object(arg));
			continue;
		}

		throw new Error(`Invalid argument ${JSON.stringify(arg)} for type ${type}`);
	}

	return normalizedArgs
}

