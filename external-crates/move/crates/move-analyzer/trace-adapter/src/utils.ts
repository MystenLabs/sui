// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Describes a Move module.
 */
export interface ModuleInfo {
    addr: string;
    name: string;
}

/**
 * If end of lifetime for a local has this value,
 * it means that it lives until the end of the current
 * frame.
 */
export const FRAME_LIFETIME = -1;

/**
 * The extension for JSON files.
 */
export const JSON_FILE_EXT = ".json";

/**
 * The extension for compressed trace files (zstd).
 */
export const COMPRESSED_FILE_EXT = ".zst";

/**
 * The extension for trace files.
 */
export const TRACE_FILE_EXT = JSON_FILE_EXT + COMPRESSED_FILE_EXT;

/**
 * Size of batches (in decompressed bytes) accumulated before line splitting.
 * Larger batches reduce per-chunk overhead; 100 MB keeps peak memory modest
 * even on machines with limited RAM.
 */
const DECOMPRESS_BATCH_SIZE = 100 * 1024 * 1024;

/**
 * Shape of the `ZSTDDecoder` class from the `zstddec/stream` package.
 * Hand-typed here because we load the package via `await import()` (its module
 * format is not directly compatible with the format this project compiles to)
 * and need a local type to work with the returned object.
 */
interface ZstdDecoder {
    init(): Promise<void>;
    decode(array: Uint8Array, uncompressedSize?: number): Uint8Array;
    decodeStreaming(arrays: Iterable<Uint8Array>): Generator<Uint8Array>;
}

let decoderPromise: Promise<ZstdDecoder> | undefined;

/**
 * Returns a lazily-initialized, cached `ZSTDDecoder` instance from
 * the `zstddec/stream` package.
 */
export async function getDecoder(): Promise<ZstdDecoder> {
    if (!decoderPromise) {
        decoderPromise = (async () => {
            const mod = await import('zstddec/stream');
            const decoder = new mod.ZSTDDecoder();
            await decoder.init();
            return decoder as unknown as ZstdDecoder;
        })();
    }
    return decoderPromise;
}

/**
 * Decompresses a Zstandard-compressed buffer.
 *
 * Accepts a `Uint8Array` rather than a file path so that it can be reused
 * unmodified by the browser-based consumer of this package.
 *
 * @param input the compressed Zstandard data.
 * @returns the decompressed bytes.
 */
export async function decompressZstd(input: Uint8Array): Promise<Uint8Array> {
    const decoder = await getDecoder();
    try {
        return decoder.decode(input);
    } catch (err) {
        const reason = err instanceof Error ? err.message : String(err);
        throw new Error(
            `Failed to decompress zstd data (${input.length} bytes): ${reason}`
        );
    }
}

/**
 * Accumulates the small decompressed chunks (~128 KB each) produced by
 * `decodeStreaming` into larger batches before yielding result to the caller.
 * This reduces the number of JS→WASM roundtrips from tens of thousands
 * to hundreds for large traces.
 */
function* batchDecompressedChunks(
    compressed: Uint8Array,
    decoder: ZstdDecoder,
    batchSize: number,
): Generator<Uint8Array> {
    // Collect small output chunks until we have at least batchSize
    // bytes, then concatenate and yield as one contiguous buffer.
    const parts: Uint8Array[] = [];
    let size = 0;
    for (const chunk of decoder.decodeStreaming([compressed])) {
        if (chunk.length === 0) continue;
        parts.push(chunk);
        size += chunk.length;
        if (size >= batchSize) {
            yield Buffer.concat(parts) as Uint8Array;
            parts.length = 0;
            size = 0;
        }
    }
    // Flush remaining chunks that didn't fill a full batch.
    if (parts.length > 0) {
        yield Buffer.concat(parts) as Uint8Array;
    }
}

/**
 * Decompresses a zstd-compressed buffer and yields its content line by line.
 * Decompressed data is accumulated into batches to amortize per-chunk
 * overhead while keeping peak memory bounded.
 *
 * If `skipLine` is provided, it is called with the raw bytes of each line
 * (as a zero-copy `Buffer` view) *before* string conversion. Lines for
 * which it returns `true` are skipped entirely — no `TextDecoder`
 * allocation, no yield.
 */
export function* streamDecompressedLines(
    compressed: Uint8Array,
    decoder: ZstdDecoder,
    skipLine?: (lineBytes: Buffer) => boolean,
): Generator<string> {
    const NEWLINE_BYTE = 0x0A;
    const td = new TextDecoder();
    // A batch boundary can split a line in the middle. This holds the
    // partial trailing bytes until the next batch completes the line.
    let leftover: Uint8Array | undefined;
    for (const batch of batchDecompressedChunks(compressed, decoder, DECOMPRESS_BATCH_SIZE)) {
        // Prepend leftover from the previous batch so that a split line
        // is reassembled before we scan for newlines.
        const buf = leftover && leftover.length > 0
            ? Buffer.concat([leftover, batch]) as Uint8Array
            : batch;
        leftover = undefined;
        // Scan for newline-delimited lines within the batch.
        let lineStart = 0;
        for (let i = 0; i < buf.length; i++) {
            if (buf[i] !== NEWLINE_BYTE) continue;
            if (i > lineStart) {
                // Zero-copy Buffer view of just this line's bytes.
                const lineBytes = Buffer.from(buf.buffer, buf.byteOffset + lineStart, i - lineStart);
                if (!skipLine || !skipLine(lineBytes)) {
                    yield td.decode(lineBytes as Uint8Array).trimEnd();
                }
            }
            lineStart = i + 1;
        }
        // Bytes after the last newline are a partial line — carry forward.
        if (lineStart < buf.length) {
            leftover = buf.subarray(lineStart);
        }
    }
    // Flush final partial line (if the file doesn't end with a newline).
    if (leftover && leftover.length > 0) {
        const lineBytes = Buffer.from(leftover.buffer, leftover.byteOffset, leftover.length);
        if (!skipLine || !skipLine(lineBytes)) {
            const tail = td.decode(lineBytes as Uint8Array).trimEnd();
            if (tail) yield tail;
        }
    }
}

