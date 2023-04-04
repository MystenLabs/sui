import { SuiAddress } from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';

export function useAddressToSuiNS(address?: SuiAddress | null) {
    return useQuery(
        ['address-to-suins', address],
        () => {
            // TODO: Remove before merging:
            // return 'what_if_the_name_is_too_long_to_include_anywhere.sui';
            return null;
        },
        // TODO: Cache + stale time:
        { enabled: !!address }
    );
}
