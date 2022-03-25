import { DefaultRpcClient as rpc } from './rpc';

export const navigateWithUnknown = async (input: string, navigate: Function) => {
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
    Promise.any([objInfoPromise, addrPromise])
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
