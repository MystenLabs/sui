import { createContext, useContext } from 'react';

export const APIContext = createContext();

export function useAPI() {
    return useContext(APIContext);
}
