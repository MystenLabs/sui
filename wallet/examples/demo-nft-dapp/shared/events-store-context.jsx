import { createContext, useContext } from 'react';

export const EventsStoreContext = createContext();

export function useSuiEvents() {
    return useContext(EventsStoreContext);
}
