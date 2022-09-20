import { createContext, useContext } from 'react';

export const ObjectsStoreContext = createContext();

export function useSuiObjects() {
    return useContext(ObjectsStoreContext);
}
