import { DisplayFieldsResponse, ObjectId } from '@mysten/sui.js';
import { useRpcClient } from '../api/RpcClientContext';
import { useQuery } from '@tanstack/react-query';
import { hasDisplayData } from '../utils/hasDisplayData';

const CHUNK_SIZE = 50;

const chunk = <T>(arr: T[] = []) =>
    Array.from({ length: Math.ceil(arr.length / CHUNK_SIZE) }, (_, i) =>
        arr.slice(i * CHUNK_SIZE, (i + 1) * CHUNK_SIZE)
    );

export function useMultiGetObjectsDisplay(ids: ObjectId[]) {
    const rpc = useRpcClient();
    return useQuery({
        queryKey: ['multiGetObjects', ids],
        queryFn: async () => {
            if (!ids) return [];
            const responses = await Promise.all(
                chunk(ids).map((chunk) =>
                    rpc.multiGetObjects({
                        ids: chunk,
                        options: {
                            showDisplay: true,
                        },
                    })
                )
            );
            return responses.flat();
        },
        select: (data) => {
            const lookup: Map<ObjectId, DisplayFieldsResponse> = new Map();
            return data.filter(hasDisplayData).reduce((acc, curr) => {
                if (curr.data?.objectId) {
                    acc.set(
                        curr.data.objectId,
                        curr.data.display as DisplayFieldsResponse
                    );
                }
                return acc;
            }, lookup);
        },
    });
}
