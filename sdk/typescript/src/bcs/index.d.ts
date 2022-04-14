export = MoveBCS;
/**
 * BCS implementation for Move types and few additional built-ins.
 */
declare function MoveBCS(): void;
declare namespace MoveBCS {
    export { BcsReader, BcsWriter, types, U8, U32, U64, U128, BOOL, ADDRESS, VECTOR, STRING, hasType, registerType, registerVectorType, registerStructType, de, ser, util, MoveBCS };
}
declare class BcsReader {
    /**
     * @param {Uint8Array} data Data to use as a buffer.
     */
    constructor(data: Uint8Array);
    /**
     * Shift current cursor position by `bytes`.
     *
     * @param {Number} bytes Number of bytes to
     * @returns {this} Self for possible chaining.
     */
    shift(bytes: number): this;
    /**
     * Read U8 value from the buffer and shift cursor by 1.
     * @returns {BN}
     */
    read8(): any;
    /**
     * Read U16 value from the buffer and shift cursor by 2.
     * @returns {BN}
     */
    read16(): any;
    /**
     * Read U32 value from the buffer and shift cursor by 4.
     * @returns {BN}
     */
    read32(): any;
    /**
     * Read U64 value from the buffer and shift cursor by 8.
     * @returns {BN}
     */
    read64(): any;
    /**
     * Read U128 value from the buffer and shift cursor by 16.
     * @returns {BN}
     */
    read128(): any;
    /**
     * Read `num` number of bytes from the buffer and shift cursor by `num`.
     * @param {Number} num Number of bytes to read.
     * @returns {Uint8Array} Selected Buffer.
     */
    readXBytes(num: number): Uint8Array;
    /**
     * Read ULEB value - an integer of varying size. Used for enum indexes and
     * vector lengths.
     * @returns {Number} The ULEB value.
     */
    readULEB(): number;
    /**
     * Read a BCS vector: read a length and then apply function `cb` X times
     * where X is the length of the vector, defined in BCS bytes.
     * @param {ReadVecCb} cb Callback to process elements of vector.
     * @returns {Array<Any>} Array of the resulting values, returned by callback.
     */
    readVec(cb: (reader: BcsReader, i: number, length: number) => any): Array<Any>;
}
declare class BcsWriter {
    /**
     * @param {Number} [size=1024] Size of the buffer to reserve for serialization.
     */
    constructor(size?: number);
    /**
     * Shift current cursor position by `bytes`.
     *
     * @param {Number} bytes Number of bytes to
     * @returns {this} Self for possible chaining.
     */
    shift(bytes: number): this;
    /**
     * Write a U8 value into a buffer and shift cursor position by 1.
     * @param {Number} value Value to write.
     * @returns {this}
     */
    write8(value: number): this;
    /**
     * Write a U16 value into a buffer and shift cursor position by 2.
     * @param {Number} value Value to write.
     * @returns {this}
     */
    write16(value: number): this;
    /**
     * Write a U32 value into a buffer and shift cursor position by 4.
     * @param {Number} value Value to write.
     * @returns {this}
     */
    write32(value: number): this;
    /**
     * Write a U64 value into a buffer and shift cursor position by 8.
     * @param {Number} value Value to write.
     * @returns {this}
     */
    write64(value: number): this;
    /**
     * Write a U128 value into a buffer and shift cursor position by 16.
     *
     * @unimplemented
     * @param {Number} value Value to write.
     * @returns {this}
     */
    write128(value: number): this;
    /**
     * Write a ULEB value into a buffer and shift cursor position by number of bytes
     * written.
     * @param {Number} value Value to write.
     * @returns {this}
     */
    writeULEB(value: number): this;
    /**
     * Write a vector into a buffer by first writing the vector length and then calling
     * a callback on each passed value.
     *
     * @param {Array<Any>} vector Array of elements to write.
     * @param {WriteVecCb} cb Callback to call on each element of the vector.
     * @returns {this}
     */
    writeVec(vector: Array<Any>, cb: (writer: BcsWriter, el: Any, i: number, length: number) => any): this;
    /**
     * Get underlying buffer taking only value bytes (in case initial buffer size was bigger).
     * @returns {Uint8Array} Resulting BCS.
     */
    toBytes(): Uint8Array;
}
declare var types: Map<string, function>;
declare var U8: string;
declare var U32: string;
declare var U64: string;
declare var U128: string;
declare var BOOL: string;
declare var ADDRESS: string;
declare var VECTOR: string;
declare var STRING: string;
declare function hasType(type: any): boolean;
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
 * BCS.registerType('number_string',
 *    (writer, data) => writer.writeVec(data, (w, el) => w.write8(el)),
 *    (reader) => reader.readVec((r) => r.read8()).join(''), // read each value as u8
 *    (value) => /[0-9]+/.test(value) // test that it has at least one digit
 * );
 * console.assert(BCS.de('number_string', BCS.ser('number_string', '12345').hex()) === '12345');
 *
 * @param {String} name
 * @param {Function} encodeCb
 * @param {Function} decodeCb Callback used to
 * @param {?Function} validateCb Optional validator Callback to check type before serialization
 */
declare function registerType(name: string, encodeCb: Function, decodeCb: Function, validateCb?: Function | null): void;
/**
 * Register custom vector type inside the MoveBCS.
 *
 * @example
 * MoveBCS.registerVectorType('vector<u8>', 'u8');
 * let array = MoveBCS.de('vector<u8>', MoveBCS.util.fromHex('06010203040506')); // [1,2,3,4,5,6];
 * let again = MoveBCS.ser('vector<u8>', [1,2,3,4,5,6]).toBytes();
 *
 * @param {String} name Name of the type to register.
 * @param {String} innerType Name of the inner type of the vector.
 * @return {Object} Returns self for chaining
 */
declare function registerVectorType(name: string, innerType: string): any;
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
 * BCS.registerStructType('Coin', {
 *   value: BCS.U64,
 *   owner: BCS.STRING,
 *   is_locked: BCS.BOOL
 * });
 *
 * // Created in Rust with diem/bcs
 * let rust_bcs_str = '80d1b105600000000e4269672057616c6c65742047757900';
 *
 * // Let's encode the value as well
 * let test_ser = BCS.ser('Coin', {
 *   owner: 'Big Wallet Guy',
 *   value: '412412400000',
 *   is_locked: false,
 * });
 *
 * console.assert(MoveBCS.util.toHex(test_ser.toArray()) === rust_bcs_str, 'Whoopsie, result mismatch');
 *
 * @param {String} name Name of the type to register.
 * @param {Object} fields Fields of the struct. Must be in the correct order.
 * @return {Object} Returns MoveBCS for chaining.
 */
declare function registerStructType(name: string, fields?: any): any;
/**
 * Deserialize BCS into a JS type.
 *
 * @example
 * // use util to form an Uint8Array buffer
 * let buffer = MoveBCS.util.fromHex('FFFFFFFF');
 * let data = MoveBCS.de(BCS.U32, buffer);
 *
 * console.assert(data == '4294967295');
 *
 * @param {String} type Name of the type to deserialize (must be registered).
 * @param {Uint8Array} data Data to deserialize.
 * @return {Number|BN|Array<Any>|Object|Boolean} Deserialized data.
 */
declare function de(type: string, data: Uint8Array): number | any | Array<Any> | any | boolean;
/**
 * Serialize data into BCS.
 *
 * @example
 * MoveBCS.registerVectorType('vector<u8>', 'u8');
 *
 * let serialized = MoveBCS
 *   .ser('vector<u8>', [1,2,3,4,5,6])
 *   .toBytes();
 *
 * console.assert(MoveBCS.util.toHex(serialized) === '06010203040506');
 *
 * @param {String} type Name of the type to serialize (must be registered).
 * @param {Uint8Array} data Data to serialize.
 * @param {Number} [size = 1024] Serialization buffer size. Default 1024 = 1KB.
 * @return {BcsReader} A BCS reader instance. Usually you'd want to call `.toBytes()`
 */
declare function ser(type: string, data: Uint8Array, size?: number): BcsReader;
declare namespace util {
    /**
     * Turn a hex string into a Uint8Array.
     *
     * @param {String} hexString A hex string.
     * @returns {Uint8Array} A buffer to use when deserializing.
     */
    function fromHex(hexString: string): Uint8Array;
    /**
     * Turn Uint8Array into a hex string (lowercased).
     *
     * @param {Uint8Array} buffer Uint8Array to encode as HEX.
     * @returns {String} hex representation of BCS.
     */
    function toHex(buffer: Uint8Array): string;
}
