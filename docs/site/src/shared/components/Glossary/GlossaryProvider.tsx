

import React, { createContext, useContext, useEffect, useMemo, useState } from "react";
import useBaseUrl from "@docusaurus/useBaseUrl";

type GlossaryMap = Map<
    string,
    { label: string; definition: string; id?: string; aliases?: string[] }
>;

const GlossaryContext = createContext<{ map: GlossaryMap | null }>({
    map: null,
});

export default function GlossaryProvider({ children }: { children: React.ReactNode }) {
    const [map, setMap] = useState<GlossaryMap | null>(null);
    const url = useBaseUrl("/glossary.json");

    useEffect(() => {
        let cancelled = false;
        (async () => {
            try {
                const res = await fetch(url);
                const data = (await res.json()) as unknown;
                const m: GlossaryMap = new Map();

                // Accept two JSON shapes:
                // 1) { "JSON API": "definition", ... }
                // 2) [ {label,id?,definition,aliases?}, ... ]
                if (Array.isArray(data)) {
                    for (const item of data as any[]) {
                        if (!item?.label || !item?.definition) continue;
                        const label: string = String(item.label);
                        const def: string = String(item.definition);
                        const id: string | undefined = item.id ? String(item.id) : undefined;
                        const aliases: string[] = Array.isArray(item.aliases)
                            ? item.aliases.map(String)
                            : [];
                        const entry = { label, definition: def, id, aliases };
                        const keys = [label, ...(id ? [id] : []), ...aliases];
                        for (const k of keys) m.set(k.toLowerCase(), entry);
                    }
                } else if (data && typeof data === "object") {
                    for (const [label, def] of Object.entries(data as Record<string, any>)) {
                        const entry = { label, definition: String(def) };
                        m.set(label.toLowerCase(), entry);
                    }
                }

                if (!cancelled) setMap(m);
            } catch (e) {
                console.error("Failed to load glossary.json", e);
                if (!cancelled) setMap(new Map());
            }
        })();
        return () => {
            cancelled = true;
        };
    }, [url]);

    const value = useMemo(() => ({ map }), [map]);
    return <GlossaryContext.Provider value={value}>{children}</GlossaryContext.Provider>;
}

export function useGlossary() {
    return useContext(GlossaryContext);
}
