export function asciiFromNumberBytes(bytes: number[]) {
    return bytes.map((n) => String.fromCharCode(n)).join('');
}

export function hexToAscii(hex: string) {
    if (!hex || typeof hex != 'string') return;
    hex = hex.replace(/^0x/, '');

    var str = '';
    for (var n = 0; n < hex.length; n += 2)
        str += String.fromCharCode(parseInt(hex.substring(n, 2), 16));

    return str;
}

const stdLibPrefix = /^0x2::/;
export const trimStdLibPrefix = (str: string): string =>
    str.replace(stdLibPrefix, '');
