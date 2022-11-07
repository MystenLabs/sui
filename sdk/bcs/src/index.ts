// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
 * BCS implementation {@see https://github.com/diem/bcs } for JavaScript.
 * Intended to be used for Move applications; supports both NodeJS and browser.
 *
 * For more details and examples {@see README.md }.
 *
 * @module bcs
 * @property {BcsReader}
 */

import { toB64, fromB64 } from "./b64";
import { toHEX, fromHEX } from "./hex";

function toLittleEndian(bigint: bigint, size: number) {
  let result = new Uint8Array(size);
  let i = 0;
  while (bigint > 0) {
    result[i] = Number(bigint % BigInt(256));
    bigint = bigint / BigInt(256);
    i += 1;
  }
  return result;
}

// Re-export all encoding dependencies.
export { toB64, fromB64, fromHEX, toHEX };

/**
 * Class used for reading BCS data chunk by chunk. Meant to be used
 * by some wrapper, which will make sure that data is valid and is
 * matching the desired format.
 *
 * @example
 * // data for this example is:
 * // { a: u8, b: u32, c: bool, d: u64 }
 *
 * let reader = new BcsReader("647f1a060001ffffe7890423c78a050102030405");
 * let field1 = reader.read8();
 * let field2 = reader.read32();
 * let field3 = reader.read8() == '1'; // bool
 * let field4 = reader.read64();
 * // ....
 *
 * Reading vectors is another deal in bcs. To read a vector, you first need to read
 * its length using {@link readULEB}. Here's an example:
 * @example
 * // data encoded: { field: [1, 2, 3, 4, 5] }
 * let reader = new BcsReader("050102030405");
 * let vec_length = reader.readULEB();
 * let elements = [];
 * for (let i = 0; i < vec_length; i++) {
 *   elements.push(reader.read8());
 * }
 * console.log(elements); // [1,2,3,4,5]
 *
 * @param {String} data HEX-encoded data (serialized BCS)
 */
export class BcsReader {
  private dataView: DataView;
  private bytePosition: number = 0;

  /**
   * @param {Uint8Array} data Data to use as a buffer.
   */
  constructor(data: Uint8Array) {
    this.dataView = new DataView(data.buffer);
  }
  /**
   * Shift current cursor position by `bytes`.
   *
   * @param {Number} bytes Number of bytes to
   * @returns {this} Self for possible chaining.
   */
  shift(bytes: number) {
    this.bytePosition += bytes;
    return this;
  }
  /**
   * Read U8 value from the buffer and shift cursor by 1.
   * @returns
   */
  read8(): number {
    let value = this.dataView.getUint8(this.bytePosition);
    this.shift(1);
    return value;
  }
  /**
   * Read U16 value from the buffer and shift cursor by 2.
   * @returns
   */
  read16(): number {
    let value = this.dataView.getUint16(this.bytePosition, true);
    this.shift(2);
    return value;
  }
  /**
   * Read U32 value from the buffer and shift cursor by 4.
   * @returns
   */
  read32(): number {
    let value = this.dataView.getUint32(this.bytePosition, true);
    this.shift(4);
    return value;
  }
  /**
   * Read U64 value from the buffer and shift cursor by 8.
   * @returns
   */
  read64(): bigint {
    let value1 = this.read32();
    let value2 = this.read32();

    let result = value2.toString(16) + value1.toString(16).padStart(8, "0");

    return BigInt("0x" + result);
  }
  /**
   * Read U128 value from the buffer and shift cursor by 16.
   */
  read128(): bigint {
    let value1 = this.read64();
    let value2 = this.read64();
    let result = value2.toString(16) + value1.toString(16).padStart(8, "0");

    return BigInt("0x" + result);
  }
  /**
   * Read U128 value from the buffer and shift cursor by 32.
   * @returns
   */
  read256(): bigint {
    let value1 = this.read128();
    let value2 = this.read128();
    let result = value2.toString(16) + value1.toString(16).padStart(16, "0");

    return BigInt("0x" + result);
  }
  /**
   * Read `num` number of bytes from the buffer and shift cursor by `num`.
   * @param num Number of bytes to read.
   */
  readBytes(num: number): Uint8Array {
    let start = this.bytePosition + this.dataView.byteOffset;
    let value = new Uint8Array(this.dataView.buffer, start, num);

    this.shift(num);

    return value;
  }
  /**
   * Read ULEB value - an integer of varying size. Used for enum indexes and
   * vector lengths.
   * @returns {Number} The ULEB value.
   */
  readULEB(): number {
    let start = this.bytePosition + this.dataView.byteOffset;
    let buffer = new Uint8Array(this.dataView.buffer, start);
    let { value, length } = ulebDecode(buffer);

    this.shift(length);

    return value;
  }
  /**
   * Read a BCS vector: read a length and then apply function `cb` X times
   * where X is the length of the vector, defined as ULEB in BCS bytes.
   * @param cb Callback to process elements of vector.
   * @returns {Array<Any>} Array of the resulting values, returned by callback.
   */
  readVec(cb: (reader: BcsReader, i: number, length: number) => any): any[] {
    let length = this.readULEB();
    let result = [];
    for (let i = 0; i < length; i++) {
      result.push(cb(this, i, length));
    }
    return result;
  }
}

/**
 * Class used to write BCS data into a buffer. Initializer requires
 * some size of a buffer to init; default value for this buffer is 1KB.
 *
 * Most methods are chainable, so it is possible to write them in one go.
 *
 * @example
 * let serialized = new BcsWriter()
 *   .write8(10)
 *   .write32(1000000)
 *   .write64(10000001000000)
 *   .hex();
 */
export class BcsWriter {
  private dataView: DataView;
  private bytePosition: number = 0;

  /**
   * @param {Number} [size=1024] Size of the buffer to reserve for serialization.
   */
  constructor(size = 1024) {
    this.dataView = new DataView(new ArrayBuffer(size));
  }

  /**
   * Shift current cursor position by `bytes`.
   *
   * @param {Number} bytes Number of bytes to
   * @returns {this} Self for possible chaining.
   */
  shift(bytes: number): this {
    this.bytePosition += bytes;
    return this;
  }
  /**
   * Write a U8 value into a buffer and shift cursor position by 1.
   * @param {Number} value Value to write.
   * @returns {this}
   */
  write8(value: number | bigint): this {
    this.dataView.setUint8(this.bytePosition, Number(value));
    return this.shift(1);
  }
  /**
   * Write a U16 value into a buffer and shift cursor position by 2.
   * @param {Number} value Value to write.
   * @returns {this}
   */
  write16(value: number | bigint): this {
    this.dataView.setUint16(this.bytePosition, Number(value), true);
    return this.shift(2);
  }
  /**
   * Write a U32 value into a buffer and shift cursor position by 4.
   * @param {Number} value Value to write.
   * @returns {this}
   */
  write32(value: number | bigint): this {
    this.dataView.setUint32(this.bytePosition, Number(value), true);
    return this.shift(4);
  }
  /**
   * Write a U64 value into a buffer and shift cursor position by 8.
   * @param {bigint} value Value to write.
   * @returns {this}
   */
  write64(value: number | bigint): this {
    toLittleEndian(BigInt(value), 8).forEach((el) => this.write8(el));

    return this;
  }
  /**
   * Write a U128 value into a buffer and shift cursor position by 16.
   *
   * @param {bigint} value Value to write.
   * @returns {this}
   */
  write128(value: number | bigint): this {
    toLittleEndian(BigInt(value), 16).forEach((el) => this.write8(el));

    return this;
  }
  /**
   * Write a U256 value into a buffer and shift cursor position by 16.
   *
   * @param {bigint} value Value to write.
   * @returns {this}
   */
  write256(value: number | bigint): this {
    toLittleEndian(BigInt(value), 32).forEach((el) => this.write8(el));

    return this;
  }
  /**
   * Write a ULEB value into a buffer and shift cursor position by number of bytes
   * written.
   * @param {Number} value Value to write.
   * @returns {this}
   */
  writeULEB(value: number): this {
    ulebEncode(value).forEach((el) => this.write8(el));
    return this;
  }
  /**
   * Write a vector into a buffer by first writing the vector length and then calling
   * a callback on each passed value.
   *
   * @param {Array<Any>} vector Array of elements to write.
   * @param {WriteVecCb} cb Callback to call on each element of the vector.
   * @returns {this}
   */
  writeVec(
    vector: any[],
    cb: (writer: BcsWriter, el: any, i: number, len: number) => {}
  ): this {
    this.writeULEB(vector.length);
    Array.from(vector).forEach((el, i) => cb(this, el, i, vector.length));
    return this;
  }

  /**
   * Adds support for iterations over the object.
   * @returns {Uint8Array}
   */
  *[Symbol.iterator](): Iterator<number, Iterable<number>> {
    for (let i = 0; i < this.bytePosition; i++) {
      yield this.dataView.getUint8(i);
    }
    return this.toBytes();
  }

  /**
   * Get underlying buffer taking only value bytes (in case initial buffer size was bigger).
   * @returns {Uint8Array} Resulting bcs.
   */
  toBytes(): Uint8Array {
    return new Uint8Array(this.dataView.buffer.slice(0, this.bytePosition));
  }

  /**
   * Represent data as 'hex' or 'base64'
   * @param encoding Encoding to use: 'base64' or 'hex'
   */
  toString(encoding: string): string {
    return encodeStr(this.toBytes(), encoding);
  }
}

// Helper utility: write number as an ULEB array.
// Original code is taken from: https://www.npmjs.com/package/uleb128 (no longer exists)
function ulebEncode(num: number): number[] {
  let arr = [];
  let len = 0;

  if (num === 0) {
    return [0];
  }

  while (num > 0) {
    arr[len] = num & 0x7f;
    if ((num >>= 7)) {
      arr[len] |= 0x80;
    }
    len += 1;
  }

  return arr;
}

// Helper utility: decode ULEB as an array of numbers.
// Original code is taken from: https://www.npmjs.com/package/uleb128 (no longer exists)
function ulebDecode(arr: number[] | Uint8Array): {
  value: number;
  length: number;
} {
  let total = 0;
  let shift = 0;
  let len = 0;

  while (true) {
    let byte = arr[len];
    len += 1;
    total |= (byte & 0x7f) << shift;
    if ((byte & 0x80) === 0) {
      break;
    }
    shift += 7;
  }

  return {
    value: total,
    length: len,
  };
}

/**
 * Set of methods that allows data encoding/decoding as standalone
 * BCS value or a part of a composed structure/vector.
 */
export interface TypeInterface {
  encode: (data: any, size: number, typeParams: string[]) => BcsWriter;
  decode: (data: Uint8Array, typeParams: string[]) => any;

  _encodeRaw: (writer: BcsWriter, data: any, typeParams: string[]) => BcsWriter;
  _decodeRaw: (reader: BcsReader, typeParams: string[]) => any;
}

/**
 * Struct type definition. Used as input format in BcsConfig.types
 * as well as an argument type for `bcs.registerStructType`.
 */
export type StructTypeDefinition = { [key: string]: string };

/**
 * Enum type definition. Used as input format in BcsConfig.types
 * as well as an argument type for `bcs.registerEnumType`.
 *
 * Value can be either `string` when invariant has a type or `null`
 * when invariant is empty.
 *
 * @example
 * bcs.registerEnumType('Option<T>', {
 *   some: 'T',
 *   none: null
 * });
 */
export type EnumTypeDefinition = { [key: string]: string | null };

/**
 * Configuration that is passed into BCS constructor.
 */
export type BcsConfig = {
  /**
   * Defines type name for the vector / array type.
   * In Move: `vector<T>` or `vector`.
   */
  vectorType: string;
  /**
   * Address length. Varies depending on a platform and
   * has to be specified for the `address` type.
   */
  addressLength: number;

  /**
   * Custom encoding for address. Supported values are
   * either 'hex' or 'base64'.
   */
  addressEncoding?: "hex" | "base64";
  /**
   * Opening and closing symbol for type parameters. Can be
   * any pair of symbols (eg `['(', ')']`); default value follows
   * Rust and Move: `<` and `>`.
   */
  genericSeparators?: [string, string];
  /**
   * Type definitions for the BCS. This field allows spawning
   * BCS instance from JSON or another prepared configuration.
   * Optional.
   */
  types?: {
    structs?: { [key: string]: StructTypeDefinition };
    enums?: { [key: string]: EnumTypeDefinition };
  };
  /**
   * Whether to auto-register primitive types on launch.
   */
  withPrimitives?: boolean;
};

/**
 * BCS implementation for Move types and few additional built-ins.
 */
export class BCS {
  // Prefefined types constants
  static readonly U8: string = "u8";
  static readonly U16: string = "u16";
  static readonly U32: string = "u32";
  static readonly U64: string = "u64";
  static readonly U128: string = "u128";
  static readonly U256: string = "u256";
  static readonly BOOL: string = "bool";
  static readonly VECTOR: string = "vector";
  static readonly ADDRESS: string = "address";
  static readonly STRING: string = "string";

  /**
   * Map of kind `TypeName => TypeInterface`. Holds all
   * callbacks for (de)serialization of every registered type.
   */
  public types: Map<string, TypeInterface> = new Map();

  /**
   * Stored BcsConfig for the current instance of BCS.
   */
  protected schema: BcsConfig;

  /**
   * Construct a BCS instance with a prepared schema.
   *
   * @param schema A prepared schema with type definitions
   * @param withPrimitives Whether to register primitive types by default
   */
  constructor(schema: BcsConfig | BCS) {
    // if BCS instance is passed -> clone its schema
    if (schema instanceof BCS) {
      this.schema = schema.schema;
      this.types = new Map(schema.types);
      return;
    }

    this.schema = schema;

    // Register address type under key 'address'.
    this.registerAddressType(
      BCS.ADDRESS,
      schema.addressLength,
      schema.addressEncoding
    );
    this.registerVectorType(schema.vectorType);

    // Register struct types if they were passed.
    if (schema.types && schema.types.structs) {
      for (let name of Object.keys(schema.types.structs)) {
        this.registerStructType(name, schema.types.structs[name]);
      }
    }

    // Register enum types if they were passed.
    if (schema.types && schema.types.enums) {
      for (let name of Object.keys(schema.types.enums)) {
        this.registerEnumType(name, schema.types.enums[name]);
      }
    }

    if (schema.withPrimitives !== false) {
      registerPrimitives(this);
    }
  }

  /**
   * Serialize data into bcs.
   *
   * @example
   * bcs.registerVectorType('vector<u8>', 'u8');
   *
   * let serialized = BCS
   *   .set('vector<u8>', [1,2,3,4,5,6])
   *   .toBytes();
   *
   * console.assert(toHex(serialized) === '06010203040506');
   *
   * @param type Name of the type to serialize (must be registered).
   * @param data Data to serialize.
   * @param size Serialization buffer size. Default 1024 = 1KB.
   * @return A BCS reader instance. Usually you'd want to call `.toBytes()`
   */
  public ser(type: string, data: any, size: number = 1024): BcsWriter {
    let { typeName, typeParams } = this.parseTypeName(type);
    return this.getTypeInterface(typeName).encode(data, size, typeParams);
  }

  /**
   * Deserialize BCS into a JS type.
   *
   * @example
   * let num = bcs.ser('u64', '4294967295').toString('hex');
   * let deNum = bcs.de('u64', num, 'hex');
   * console.assert(deNum.toString(10) === '4294967295');
   *
   * @param type Name of the type to deserialize (must be registered).
   * @param data Data to deserialize.
   * @param encoding Optional - encoding to use if data is of type String
   * @return Deserialized data.
   */
  public de(type: string, data: Uint8Array | string, encoding?: string): any {
    if (typeof data == "string") {
      if (encoding) {
        data = decodeStr(data, encoding);
      } else {
        throw new Error("To pass a string to `bcs.de`, specify encoding");
      }
    }

    let { typeName, typeParams } = this.parseTypeName(type);
    return this.getTypeInterface(typeName).decode(data, typeParams);
  }

  /**
   * Check whether a `TypeInterface` has been loaded for a `type`.
   * @param type Name of the type to check.
   * @returns
   */
  public hasType(type: string): boolean {
    return this.types.has(type);
  }

  /**
   * Method to register new types for BCS internal representation.
   * For each registered type 2 callbacks must be specified and one is optional:
   *
   * - encodeCb(writer, data) - write a way to serialize data with BcsWriter;
   * - decodeCb(reader) - write a way to deserialize data with BcsReader;
   * - validateCb(data) - validate data - either return bool or throw an error
   *
   * @example
   * // our type would be a string that consists only of numbers
   * bcs.registerType('number_string',
   *    (writer, data) => writer.writeVec(data, (w, el) => w.write8(el)),
   *    (reader) => reader.readVec((r) => r.read8()).join(''), // read each value as u8
   *    (value) => /[0-9]+/.test(value) // test that it has at least one digit
   * );
   * console.log(Array.from(bcs.ser('number_string', '12345').toBytes()) == [5,1,2,3,4,5]);
   *
   * @param name
   * @param encodeCb Callback to encode a value.
   * @param decodeCb Callback to decode a value.
   * @param validateCb Optional validator Callback to check type before serialization.
   */
  public registerType(
    name: string,
    encodeCb: (writer: BcsWriter, data: any, typeParams: string[]) => BcsWriter,
    decodeCb: (reader: BcsReader, typeParams: string[]) => any,
    validateCb: (data: any) => boolean = () => true
  ): BCS {
    this.types.set(name, {
      encode(data, size = 1024, typeParams) {
        return this._encodeRaw(new BcsWriter(size), data, typeParams);
      },
      decode(data, typeParams) {
        return this._decodeRaw(new BcsReader(data), typeParams);
      },

      // these methods should always be used with caution as they require pre-defined
      // reader and writer and mainly exist to allow multi-field (de)serialization;
      _encodeRaw(writer, data, typeParams) {
        if (validateCb(data)) {
          return encodeCb(writer, data, typeParams);
        } else {
          throw new Error(`Validation failed for type ${name}, data: ${data}`);
        }
      },
      _decodeRaw(reader, typeParams) {
        return decodeCb(reader, typeParams);
      },
    });

    return this;
  }

  /**
   * Register an address type which is a sequence of U8s of specified length.
   * @example
   * bcs.registerAddressType('address', 20);
   * let addr = bcs.de('address', 'ca27601ec5d915dd40d42e36c395d4a156b24026');
   *
   * @param name Name of the address type.
   * @param length Byte length of the address.
   * @param encoding Encoding to use for the address type
   * @returns
   */
  public registerAddressType(
    name: string,
    length: number,
    encoding: string | void = "hex"
  ): BCS {
    switch (encoding) {
      case "base64":
        return this.registerType(
          name,
          (writer, data: string) =>
            fromB64(data).reduce((writer, el) => writer.write8(el), writer),
          (reader) => toB64(reader.readBytes(length))
        );
      case "hex":
        return this.registerType(
          name,
          (writer, data: string) =>
            fromHEX(data).reduce((writer, el) => writer.write8(el), writer),
          (reader) => toHEX(reader.readBytes(length))
        );
      default:
        throw new Error("Unsupported encoding! Use either hex or base64");
    }
  }

  /**
   * Register custom vector type inside the bcs.
   *
   * @example
   * bcs.registerVectorType('vector<T>'); // generic registration
   * let array = bcs.de('vector<u8>', '06010203040506', 'hex'); // [1,2,3,4,5,6];
   * let again = bcs.ser('vector<u8>', [1,2,3,4,5,6]).toString('hex');
   *
   * @param name Name of the type to register
   * @param elementType Optional name of the inner type of the vector
   * @return Returns self for chaining.
   */
  private registerVectorType(name: string, elementType?: string): BCS {
    let { typeName, typeParams } = this.parseTypeName(name);
    if (typeParams.length > 1) {
      throw new Error("Vector can have only one type parameter; got " + name);
    }

    return this.registerType(
      typeName,
      (writer: BcsWriter, data: any[], typeParams: string[]) =>
        writer.writeVec(data, (writer, el) => {
          let vectorType = elementType || typeParams[0];

          if (!!vectorType) {
            let { typeName, typeParams } = this.parseTypeName(vectorType);
            return this.getTypeInterface(elementType || typeName)._encodeRaw(
              writer,
              el,
              typeParams
            );
          } else {
            throw new Error(
              `Incorrect number of type parameters passed to vector '${typeName}'`
            );
          }
        }),
      (reader: BcsReader, typeParams) =>
        reader.readVec((reader) => {
          let vectorType = elementType || typeParams[0];
          if (!!vectorType) {
            let { typeName, typeParams } = this.parseTypeName(vectorType);
            return this.getTypeInterface(elementType || typeName)._decodeRaw(
              reader,
              typeParams
            );
          } else {
            throw new Error(
              `Incorrect number of type parameters passed to vector '${typeName}'`
            );
          }
        })
    );
  }

  /**
   * Safe method to register a custom Move struct. The first argument is a name of the
   * struct which is only used on the FrontEnd and has no affect on serialization results,
   * and the second is a struct description passed as an Object.
   *
   * The description object MUST have the same order on all of the platforms (ie in Move
   * or in Rust).
   *
   * @example
   * // Move / Rust struct
   * // struct Coin {
   * //   value: u64,
   * //   owner: vector<u8>, // name // Vec<u8> in Rust
   * //   is_locked: bool,
   * // }
   *
   * bcs.registerStructType('Coin', {
   *   value: bcs.U64,
   *   owner: bcs.STRING,
   *   is_locked: bcs.BOOL
   * });
   *
   * // Created in Rust with diem/bcs
   * // let rust_bcs_str = '80d1b105600000000e4269672057616c6c65742047757900';
   * let rust_bcs_str = [ // using an Array here as BCS works with Uint8Array
   *  128, 209, 177,   5,  96,  0,  0,
   *    0,  14,  66, 105, 103, 32, 87,
   *   97, 108, 108, 101, 116, 32, 71,
   *  117, 121,   0
   * ];
   *
   * // Let's encode the value as well
   * let test_set = bcs.ser('Coin', {
   *   owner: 'Big Wallet Guy',
   *   value: '412412400000',
   *   is_locked: false,
   * });
   *
   * console.assert(Array.from(test_set.toBytes()) === rust_bcs_str, 'Whoopsie, result mismatch');
   *
   * @param name Name of the type to register.
   * @param fields Fields of the struct. Must be in the correct order.
   * @return Returns BCS for chaining.
   */
  public registerStructType(name: string, fields: StructTypeDefinition): BCS {
    let struct = Object.freeze(fields); // Make sure the order doesn't get changed

    // IMPORTANT: we need to store canonical order of fields for each registered
    // struct so we maintain it and allow developers to use any field ordering in
    // their code (and not cause mismatches based on field order).
    let canonicalOrder = Object.keys(struct);

    // Holds generics for the struct definition. At this stage we can check that
    // generic parameter matches the one defined in the struct.
    let { typeName, typeParams: structTypeParams } = this.parseTypeName(name);

    // Make sure all the types in the fields description are already known
    // and that all the field types are strings.
    return this.registerType(
      typeName,
      // (typeParam: string) => ,
      (writer: BcsWriter, data: { [key: string]: any }, typeParams) => {
        if (!data || data.constructor !== Object) {
          throw new Error(`Expected ${name} to be an Object, got: ${data}`);
        }
        for (let key of canonicalOrder) {
          if (key in data) {
            let paramIndex = structTypeParams.indexOf(struct[key]);
            let typeOrParam =
              paramIndex === -1 ? struct[key] : typeParams[paramIndex];
            {
              let { typeName, typeParams } = this.parseTypeName(typeOrParam);
              this.getTypeInterface(typeName)._encodeRaw(
                writer,
                data[key],
                typeParams
              );
            }
          } else {
            throw new Error(
              `Struct ${name} requires field ${key}:${struct[key]}`
            );
          }
        }
        return writer;
      },
      (reader: BcsReader, typeParams) => {
        let result: { [key: string]: any } = {};
        for (let key of canonicalOrder) {
          let paramIndex = structTypeParams.indexOf(struct[key]);
          let typeOrParam =
            paramIndex === -1 ? struct[key] : typeParams[paramIndex];
          {
            let { typeName, typeParams } = this.parseTypeName(typeOrParam);
            result[key] = this.getTypeInterface(typeName)._decodeRaw(
              reader,
              typeParams
            );
          }
        }
        return result;
      }
    );
  }

  /**
   * Safe method to register custom enum type where each invariant holds the value of another type.
   * @example
   * bcs.registerStructType('Coin', { value: 'u64' });
   * bcs.registerEnumType('MyEnum', {
   *  single: 'Coin',
   *  multi: 'vector<Coin>',
   *  empty: null
   * });
   *
   * console.log(
   *  bcs.de('MyEnum', 'AICWmAAAAAAA', 'base64'), // { single: { value: 10000000 } }
   *  bcs.de('MyEnum', 'AQIBAAAAAAAAAAIAAAAAAAAA', 'base64')  // { multi: [ { value: 1 }, { value: 2 } ] }
   * )
   *
   * // and serialization
   * bcs.ser('MyEnum', { single: { value: 10000000 } }).toBytes();
   * bcs.ser('MyEnum', { multi: [ { value: 1 }, { value: 2 } ] });
   *
   * @param name
   * @param variants
   */
  public registerEnumType(name: string, variants: EnumTypeDefinition): BCS {
    let struct = Object.freeze(variants); // Make sure the order doesn't get changed

    // IMPORTANT: enum is an ordered type and we have to preserve ordering in BCS
    let canonicalOrder = Object.keys(struct);

    // Parse type parameters in advance to know the index of each generic parameter.
    let { typeName, typeParams: canonicalTypeParams } =
      this.parseTypeName(name);

    return this.registerType(
      typeName,
      (writer: BcsWriter, data: { [key: string]: any | null }, typeParams) => {
        if (data === undefined) {
          throw new Error(`Unable to write enum ${name}, missing data`);
        }
        let key = Object.keys(data)[0];
        if (key === undefined) {
          throw new Error(`Unknown invariant of the enum ${name}`);
        }

        let orderByte = canonicalOrder.indexOf(key);
        if (orderByte === -1) {
          throw new Error(
            `Unknown invariant of the enum ${name}, allowed values: ${canonicalOrder}`
          );
        }
        let invariant = canonicalOrder[orderByte];
        let invariantType = struct[invariant];

        // write order byte
        writer.write8(orderByte);

        // When { "key": null } - empty value for the invariant.
        if (invariantType === null) {
          return writer;
        }

        let paramIndex = canonicalTypeParams.indexOf(invariantType);
        let typeOrParam =
          paramIndex === -1 ? invariantType : typeParams[paramIndex];

        {
          let { typeName, typeParams } = this.parseTypeName(typeOrParam);
          return this.getTypeInterface(typeName)._encodeRaw(
            writer,
            data[key],
            typeParams
          );
        }
      },
      (reader: BcsReader, typeParams) => {
        let orderByte = reader.readULEB();
        let invariant = canonicalOrder[orderByte];
        let invariantType = struct[invariant];

        if (orderByte === -1) {
          throw new Error(
            `Decoding type mismatch, expected enum ${name} invariant index, received ${orderByte}`
          );
        }

        // Encode an empty value for the enum.
        if (invariantType === null) {
          return { [invariant]: true };
        }

        let paramIndex = canonicalTypeParams.indexOf(invariantType);
        let typeOrParam =
          paramIndex === -1 ? invariantType : typeParams[paramIndex];

        {
          let { typeName, typeParams } = this.parseTypeName(typeOrParam);
          return {
            [invariant]: this.getTypeInterface(typeName)._decodeRaw(
              reader,
              typeParams
            ),
          };
        }
      }
    );
  }
  /**
   * Get a set of encoders/decoders for specific type.
   * Mainly used to define custom type de/serialization logic.
   *
   * @param type
   * @returns {TypeInterface}
   */
  public getTypeInterface(type: string): TypeInterface {
    let typeInterface = this.types.get(type);
    if (typeInterface === undefined) {
      throw new Error(`Type ${type} is not registered`);
    }

    return typeInterface;
  }

  /**
   * Parse a type name and get the type's generics.
   * @example
   * let { typeName, typeParams } = parseTypeName('Option<Coin<SUI>>');
   * // typeName: Option
   * // typeParams: [ 'Coin<SUI>' ]
   *
   * @param name Name of the type to process
   * @returns Object with typeName and typeParams listed as Array
   */
  public parseTypeName(name: string): {
    typeName: string;
    typeParams: string[];
  } {
    let [left, right] = this.schema.genericSeparators || ["<", ">"];

    let l_bound = name.indexOf(left);
    let r_bound = Array.from(name).reverse().indexOf(right);

    // if there are no generics - exit gracefully.
    if (l_bound === -1 && r_bound === -1) {
      return { typeName: name, typeParams: [] };
    }

    // if one of the bounds is not defined - throw an Error.
    if (l_bound === -1 || r_bound === -1) {
      throw new Error(`Unclosed generic in name '${name}'`);
    }

    let typeName = name.slice(0, l_bound);
    let typeParams = name
      .slice(l_bound + 1, name.length - r_bound - 1)
      .split(",")
      .map((e) => e.trim());

    return { typeName, typeParams };
  }
}

/**
 * Encode data with either `hex` or `base64`.
 *
 * @param {Uint8Array} data Data to encode.
 * @param {String} encoding Encoding to use: base64 or hex
 * @return {String} Encoded value.
 */
export function encodeStr(data: Uint8Array, encoding: string): string {
  switch (encoding) {
    case "base64":
      return toB64(data);
    case "hex":
      return toHEX(data);
    default:
      throw new Error(
        "Unsupported encoding, supported values are: base64, hex"
      );
  }
}

/**
 * Decode either `base64` or `hex` data.
 *
 * @param {String} data Data to encode.
 * @param {String} encoding Encoding to use: base64 or hex
 * @return {Uint8Array} Encoded value.
 */
export function decodeStr(data: string, encoding: string): Uint8Array {
  switch (encoding) {
    case "base64":
      return fromB64(data);
    case "hex":
      return fromHEX(data);
    default:
      throw new Error(
        "Unsupported encoding, supported values are: base64, hex"
      );
  }
}

/**
 * Register the base set of primitive and common types.
 * Is called in the `BCS` contructor automatically but can
 * be ignored if the `withPrimitives` argument is not set.
 */
export function registerPrimitives(bcs: BCS): void {
  bcs.registerType(
    BCS.U8,
    (writer: BcsWriter, data) => writer.write8(data),
    (reader: BcsReader) => reader.read8(),
    (u8) => u8 < 256
  );

  bcs.registerType(
    BCS.U16,
    (writer: BcsWriter, data) => writer.write16(data),
    (reader: BcsReader) => reader.read16(),
    (u16) => u16 < 65536
  );

  bcs.registerType(
    BCS.U32,
    (writer: BcsWriter, data) => writer.write32(data),
    (reader: BcsReader) => reader.read32(),
    (u32) => u32 <= 4294967296
  );

  bcs.registerType(
    BCS.U64,
    (writer: BcsWriter, data) => writer.write64(data),
    (reader: BcsReader) => reader.read64(),
    (_u64) => true
  );

  bcs.registerType(
    BCS.U128,
    (writer: BcsWriter, data: bigint) => writer.write128(data),
    (reader: BcsReader) => reader.read128(),
    (_u128) => true
  );

  bcs.registerType(
    BCS.U256,
    (writer: BcsWriter, data) => writer.write256(data),
    (reader: BcsReader) => reader.read256(),
    (_u256) => true
  );

  bcs.registerType(
    BCS.BOOL,
    (writer: BcsWriter, data) => writer.write8(data),
    (reader: BcsReader) => reader.read8().toString(10) === "1",
    (_bool: boolean) => true
  );

  bcs.registerType(
    BCS.STRING,
    (writer: BcsWriter, data: string) =>
      writer.writeVec(Array.from(data), (writer, el) =>
        writer.write8(el.charCodeAt(0))
      ),
    (reader: BcsReader) => {
      return reader
        .readVec((reader) => reader.read8())
        .map((el: bigint) => String.fromCharCode(Number(el)))
        .join("");
    },
    (_str: string) => true
  );
}

export function getRustConfig(): BcsConfig {
  return {
    genericSeparators: ["<", ">"],
    vectorType: "Vec",
    addressLength: 20,
    addressEncoding: "hex",
  };
}

export function getSuiMoveConfig(): BcsConfig {
  return {
    genericSeparators: ["<", ">"],
    vectorType: "vector",
    addressLength: 20,
    addressEncoding: "hex",
  };
}
