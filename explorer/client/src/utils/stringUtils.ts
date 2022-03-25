export const asciiFromNumberBytes = (bytes: number[]): string =>
    bytes.map((n) => String.fromCharCode(n)).join('');

export const trimStdLibPrefix = (str: string): string =>
    str.replace(/^0x2::/, '');
