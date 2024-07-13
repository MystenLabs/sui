# BCS - Binary Canonical Serialization

This small and lightweight library implements
[Binary Canonical Serialization (BCS)](https://github.com/zefchain/bcs) in TypeScript, making BCS
available in both Browser and NodeJS environments in a type-safe way.`

## Install

To install, add the [`@mysten/bcs`](https://www.npmjs.com/package/@mysten/bcs) package to your
project:

```sh npm2yarn
npm i @mysten/bcs
```

## Quickstart

```ts
import { bcs } from '@mysten/bcs';

// define UID as a 32-byte array, then add a transform to/from hex strings
const UID = bcs.fixedArray(32, bcs.u8()).transform({
	input: (id: string) => fromHEX(id),
	output: (id) => toHEX(Uint8Array.from(id)),
});

const Coin = bcs.struct('Coin', {
	id: UID,
	value: bcs.u64(),
});

// deserialization: BCS bytes into Coin
const bcsBytes = Coin.serialize({
	id: '0000000000000000000000000000000000000000000000000000000000000001',
	value: 1000000n,
}).toBytes();

const coin = Coin.parse(bcsBytes);

// serialization: Object into bytes - an Option with <T = Coin>
const hex = bcs.option(Coin).serialize(coin).toHex();

console.log(hex);
```

## Description

BCS defines the way the data is serialized, and the serialized results contains no type information.
To be able to serialize the data and later deserialize it, a schema has to be created (based on the
built-in primitives, such as `string` or `u64`). There are no type hints in the serialized bytes on
what they mean, so the schema used for decoding must match the schema used to encode the data.

The `@mysten/bcs` library can be used to define schemas that can serialize and deserialize BCS
encoded data, and can infer the correct TypeScript for the schema from the definitions themselves
rather than having to define them manually.

## Basic types

bcs supports a number of built in base types that can be combined to create more complex types. The
following table lists the primitive types available:

| Method                | TS Type      | TS Input Type                | Description                                                                 |
| --------------------- | ------------ | ---------------------------- | --------------------------------------------------------------------------- |
| `bool`                | `boolean`    | `boolean`                    | Boolean type (converts to `true` / `false`)                                 |
| `u8`, `u16`, `u32`    | `number`     | `number`                     | Unsigned Integer types                                                      |
| `u64`, `u128`, `u256` | `string`     | `number \| string \| bigint` | Unsigned Integer types, decoded as `string` to allow for JSON serialization |
| `uleb128`             | `number`     | `number`                     | Unsigned LEB128 integer type                                                |
| `string`              | `string`     | `string`                     | UTF-8 encoded string                                                        |
| `bytes(size)`         | `Uint8Array` | `Iterable<number>`           | Fixed length bytes                                                          |

```ts
import { bcs } from '@mysten/bcs';

// Integers
const u8 = bcs.u8().serialize(100).toBytes();
const u64 = bcs.u64().serialize(1000000n).toBytes();
const u128 = bcs.u128().serialize('100000010000001000000').toBytes();

// Other types
const str = bcs.string().serialize('this is an ascii string').toBytes();
const hex = bcs.hex().serialize('C0FFEE').toBytes();
const bytes = bcs.bytes(4).serialize([1, 2, 3, 4]).toBytes();

// Parsing data back into original types
const parsedU8 = bcs.u8().parse(u8);
// u64-u256 will be represented as bigints regardless of how they were provided when serializing them
const parsedU64 = bcs.u64().parse(u64);
const parsedU128 = bcs.u128().parse(u128);

const parsedStr = bcs.string().parse(str);
const parsedHex = bcs.hex().parse(hex);
const parsedBytes = bcs.bytes(4).parse(bytes);
```

## Compound types

For most use-cases you'll want to combine primitive types into more complex types like `vectors`,
`structs` and `enums`. The following table lists methods available for creating compound types:

| Method                 | Description                                           |
| ---------------------- | ----------------------------------------------------- |
| `vector(type: T)`      | A variable length list of values of type `T`          |
| `fixedArray(size, T)`  | A fixed length array of values of type `T`            |
| `option(type: T)`      | A value of type `T` or `null`                         |
| `enum(name, values)`   | An enum value representing one of the provided values |
| `struct(name, fields)` | A struct with named fields of the provided types      |
| `tuple(types)`         | A tuple of the provided types                         |
| `map(K, V)`            | A map of keys of type `K` to values of type `V`       |

```ts
import { bcs } from '@mysten/bcs';

// Vectors
const intList = bcs.vector(bcs.u8()).serialize([1, 2, 3, 4, 5]).toBytes();
const stringList = bcs.vector(bcs.string()).serialize(['a', 'b', 'c']).toBytes();

// Arrays
const intArray = bcs.array(4, bcs.u8()).serialize([1, 2, 3, 4]).toBytes();
const stringArray = bcs.array(3, bcs.string()).serialize(['a', 'b', 'c']).toBytes();

// Option
const option = bcs.option(bcs.string()).serialize('some value').toBytes();
const nullOption = bcs.option(bcs.string()).serialize(null).toBytes();

// Enum
const MyEnum = bcs.enum('MyEnum', {
	NoType: null,
	Int: bcs.u8(),
	String: bcs.string(),
	Array: bcs.array(3, bcs.u8()),
});

const noTypeEnum = MyEnum.serialize({ NoType: null }).toBytes();
const intEnum = MyEnum.serialize({ Int: 100 }).toBytes();
const stringEnum = MyEnum.serialize({ String: 'string' }).toBytes();
const arrayEnum = MyEnum.serialize({ Array: [1, 2, 3] }).toBytes();

// Struct
const MyStruct = bcs.struct('MyStruct', {
	id: bcs.u8(),
	name: bcs.string(),
});

const struct = MyStruct.serialize({ id: 1, name: 'name' }).toBytes();

// Tuple
const tuple = bcs.tuple([bcs.u8(), bcs.string()]).serialize([1, 'name']).toBytes();

// Map
const map = bcs
	.map(bcs.u8(), bcs.string())
	.serialize(
		new Map([
			[1, 'one'],
			[2, 'two'],
		]),
	)
	.toBytes();

// Parsing data back into original types

// Vectors
const parsedIntList = bcs.vector(bcs.u8()).parse(intList);
const parsedStringList = bcs.vector(bcs.string()).parse(stringList);

// Arrays
const parsedIntArray = bcs.array(4, bcs.u8()).parse(intArray);

// Option
const parsedOption = bcs.option(bcs.string()).parse(option);
const parsedNullOption = bcs.option(bcs.string()).parse(nullOption);

// Enum
const parsedNoTypeEnum = MyEnum.parse(noTypeEnum);
const parsedIntEnum = MyEnum.parse(intEnum);
const parsedStringEnum = MyEnum.parse(stringEnum);
const parsedArrayEnum = MyEnum.parse(arrayEnum);

// Struct
const parsedStruct = MyStruct.parse(struct);

// Tuple
const parsedTuple = bcs.tuple([bcs.u8(), bcs.string()]).parse(tuple);

// Map
const parsedMap = bcs.map(bcs.u8(), bcs.string()).parse(map);
```

## Generics

To define a generic struct or an enum, you can define a generic typescript function helper

```ts
// Example: Generics
import { bcs, BcsType } from '@mysten/bcs';

// The T typescript generic is a placeholder for the typescript type of the generic value
// The T argument will be the bcs type passed in when creating a concrete instance of the Container type
function Container<T>(T: BcsType<T>) {
	return bcs.struct('Container<T>', {
		contents: T,
	}),
}

// When serializing, we have to pass the type to use for `T`
const bytes = Container(bcs.u8()).serialize({ contents: 100 }).toBytes();

// Alternatively we can save the concrete type as a variable
const U8Container = Container(bcs.u8());
const bytes = U8Container.serialize({ contents: 100 }).toBytes();

// Using multiple generics
function VecMap<K, V>, (K: BcsType<K>, V: BcsType<V>) {
	// You can use the names of the generic params in the type name to
	return bcs.struct(
		// You can use the names of the generic params to give your type a more useful name
		`VecMap<${K.name}, ${V.name}>`,
		{
			keys: bcs.vector(K),
			values: bcs.vector(V),
		}
	)
}

// To serialize VecMap, we can use:
VecMap(bcs.string(), bcs.string())
	.serialize({
		keys: ['key1', 'key2', 'key3'],
		values: ['value1', 'value2', 'value3'],
	})
	.toBytes();
```

## Transforms

If you the format you use in your code is different from the format expected for BCS serialization,
you can use the `transform` API to map between the types you use in your application, and the types
needed for serialization.

The `address` type used by Move code is a good example of this. In many cases, you'll want to
represent an address as a hex string, but the BCS serialization format for addresses is a 32 byte
array. To handle this, you can use the `transform` API to map between the two formats:

```ts
const Address = bcs.bytes(32).transform({
	// To change the input type, you need to provide a type definition for the input
	input: (val: string) => fromHEX(val),
	output: (val) => toHEX(val),
});

const serialized = Address.serialize('0x000000...').toBytes();
const parsed = Address.parse(serialized); // will return a hex string
```

When using a transform, a new type is created that uses the inferred return value of `output` as the
return type of the `parse` method, and the type of the `input` argument as the allowed input type
when calling `serialize`. The `output` type can generally be inferred from the definition, but the
input type will need to be provided explicitly. In some cases, for complex transforms, you'll need
to manually type the return

transforms can also handle more complex types. For instance, `@mysten/sui` uses the following
definition to transform enums into a more TypeScript friends format:

```ts
type Merge<T> = T extends infer U ? { [K in keyof U]: U[K] } : never;
type EnumKindTransform<T> = T extends infer U
	? Merge<(U[keyof U] extends null | boolean ? object : U[keyof U]) & { kind: keyof U }>
	: never;

function enumKind<T extends object, Input extends object>(type: BcsType<T, Input>) {
	return type.transform({
		input: ({ kind, ...val }: EnumKindTransform<Input>) =>
			({
				[kind]: val,
			}) as Input,
		output: (val) => {
			const key = Object.keys(val)[0] as keyof T;

			return { kind: key, ...val[key] } as EnumKindTransform<T>;
		},
	});
}

const MyEnum = enumKind(
	bcs.enum('MyEnum', {
		A: bcs.struct('A', {
			id: bcs.u8(),
		}),
		B: bcs.struct('B', {
			val: bcs.string(),
		}),
	}),
);

// Enums wrapped with enumKind flatten the enum variants and add a `kind` field to differentiate them
const A = MyEnum.serialize({ kind: 'A', id: 1 }).toBytes();
const B = MyEnum.serialize({ kind: 'B', val: 'xyz' }).toBytes();

const parsedA = MyEnum.parse(A); // returns { kind: 'A', id: 1 }
```

## Formats for serialized bytes

When you call `serialize` on a `BcsType`, you will receive a `SerializedBcs` instance. This wrapper
preserves type information for the serialized bytes, and can be used to get raw data in various
formats.

```ts
import { bcs, fromB58, fromB64, fromHex } from '@mysten/bcs';

const serializedString = bcs.string().serialize('this is a string');

// SerializedBcs.toBytes() returns a Uint8Array
const bytes: Uint8Array = serializedString.toBytes();

// You can get the serialized bytes encoded as hex, base64 or base58
const hex: string = serializedString.toHex();
const base64: string = bcsWriter.toBase64();
const base58: string = bcsWriter.toBase58();

// To parse a BCS value from bytes, the bytes need to be a Uint8Array
const str1 = bcs.string().parse(bytes);

// If your data is encoded as string, you need to convert it to Uint8Array first
const str2 = bcs.string().parse(fromHex(hex));
const str3 = bcs.string().parse(fromB64(base64));
const str4 = bcs.string().parse(fromB58(base58));

console.assert((str1 == str2) == (str3 == str4), 'Result is the same');
```

## Inferring types

BCS types have both a `type` and an `inputType`. For some types these are the same, but for others
(like `u64`) the types diverge slightly to make inputs more flexible. For instance, `u64` will
always be `string` for it's type, but can be a `number`, `string` or `bigint` for it's input type.

You can infer these types in one of 2 ways, either using the `$inferType` and `$inferInput`
properties on a `BcsType`, or using the `InferBcsType` and `InferBcsInput` type helpers.

```ts
import { bcs, type InferBcsType, type InferBcsInput } from '@mysten/bcs';

const MyStruct = bcs.struct('MyStruct', {
	id: bcs.u64(),
	name: bcs.string(),
});

// using the $inferType and $inferInput properties
type MyStructType = typeof MyStruct.$inferType; // { id: string; name: string; }
type MyStructInput = typeof MyStruct.$inferInput; // { id: number | string | bigint; name: string; }

// using the InferBcsType and InferBcsInput type helpers
type MyStructType = InferBcsType<typeof MyStruct>; // { id: string; name: string; }
type MyStructInput = InferBcsInput<typeof MyStruct>; // { id: number | string | bigint; name: string; }
```
