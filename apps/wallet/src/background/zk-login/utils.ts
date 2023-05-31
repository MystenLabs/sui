import {
    poseidon1,
    poseidon2,
    poseidon3,
    poseidon4,
    poseidon5,
    poseidon6,
    poseidon7,
    poseidon8,
    poseidon9,
    poseidon10,
    poseidon11,
    poseidon12,
    poseidon13,
    poseidon14,
    poseidon15,
} from 'poseidon-lite';

const maxKeyClaimValueLength = 50;
const packWidth = 248;
const poseidonNumToHashFN = [
    undefined,
    poseidon1,
    poseidon2,
    poseidon3,
    poseidon4,
    poseidon5,
    poseidon6,
    poseidon7,
    poseidon8,
    poseidon9,
    poseidon10,
    poseidon11,
    poseidon12,
    poseidon13,
    poseidon14,
    poseidon15,
];

function getNumFieldElements(asciiSize: number) {
    if (packWidth % 8 !== 0)
        throw new Error('packWidth must be a multiple of 8');
    const packWidthInBytes = packWidth / 8;
    return Math.ceil(asciiSize / packWidthInBytes);
}

function arrayChunk(array: unknown[], chunkSize: number) {
    return Array(Math.ceil(array.length / chunkSize))
        .fill(0)
        .map((_, index) => index * chunkSize)
        .map((begin) => array.slice(begin, begin + chunkSize));
}

function bigIntArray2Bits(arr: bigint[], intSize = 16) {
    return arr.reduce<number[]>((bitArray, n) => {
        const binaryString = n.toString(2).padStart(intSize, '0');
        const bitValues = binaryString.split('').map(Number);
        return bitArray.concat(bitValues);
    }, []);
}

// Pack into an array of chunks each outWidth bits
function pack(inArr: bigint[], inWidth: number, outWidth: number) {
    const bits = bigIntArray2Bits(inArr, inWidth);

    const extra_bits =
        bits.length % outWidth === 0 ? 0 : outWidth - (bits.length % outWidth);
    const bits_padded = bits.concat(Array(extra_bits).fill(0));
    if (bits_padded.length % outWidth !== 0) throw new Error('Invalid logic');

    const packed = arrayChunk(bits_padded, outWidth).map((chunk) =>
        BigInt('0b' + chunk.join(''))
    );
    return packed;
}

// Pack into exactly outCount chunks of outWidth bits each
function pack2(
    inArr: bigint[],
    inWidth: number,
    outWidth: number,
    outCount: number
) {
    const packed = pack(inArr, inWidth, outWidth);
    if (packed.length > outCount) throw new Error('packed is big enough');

    return packed.concat(Array(outCount - packed.length).fill(0));
}

export function poseidonHash(inputs: (string | number | bigint)[]): bigint {
    const hashFN = poseidonNumToHashFN[inputs.length];
    if (hashFN) {
        return hashFN(inputs);
    } else if (inputs.length <= 30) {
        const hash1 = poseidonHash(inputs.slice(0, 15));
        const hash2 = poseidonHash(inputs.slice(15));
        return poseidonHash([hash1, hash2]);
    } else {
        throw new Error(
            `Yet to implement: Unable to hash a vector of length ${inputs.length}`
        );
    }
}

// Map str into a field element after padding it to maxSize chars
async function mapToField(value: string, maxSize: number) {
    if (value.length > maxSize) {
        throw new Error(`String ${value} is longer than ${maxSize} chars`);
    }

    const numElements = getNumFieldElements(maxSize);
    const packed = pack2(
        value.split('').map((c) => BigInt(c.charCodeAt(0))),
        8,
        packWidth,
        numElements
    );
    return poseidonHash(packed);
}

export async function getAddressSeed(claimValue: string, pin: bigint) {
    return poseidonHash([
        await mapToField(claimValue, maxKeyClaimValueLength),
        pin,
    ]);
}
