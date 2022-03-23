import { DefaultRpcClient as rpc } from './rpc';

const navigateWithUnknown = async (input: string, navigate: Function) => {
    // feels crude to just search each category for an ID, but works for now
    const addrPromise = rpc.getAddressObjects(input).then((data) => {
        if (data.length > 0) {
            return {
                category: 'addresses',
                data: data,
            };
        } else {
            throw new Error('No objects for Address');
        }
    });

    const objInfoPromise = rpc.getObjectInfo(input).then((data) => ({
        category: 'objects',
        data: data,
    }));

    //if none of the queries find a result, show missing page
    return Promise.any([objInfoPromise, addrPromise])
        .then((pac) => {
            if (
                pac?.data &&
                (pac?.category === 'objects' || pac?.category === 'addresses')
            ) {
                navigate(`../${pac.category}/${input}`, { state: pac.data });
            } else {
                throw new Error(
                    'Something wrong with navigateWithUnknown function'
                );
            }
        })
        .catch((error) => {
            console.log(error);
            navigate(`../missing/${input}`);
        });
};

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

export { navigateWithUnknown };
