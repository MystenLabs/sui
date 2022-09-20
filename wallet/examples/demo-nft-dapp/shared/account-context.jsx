import { createContext, useContext } from 'react';

export const AccountContext = createContext();

export function useAccount() {
    return useContext(AccountContext);
}
