import { PostHogConfig } from 'posthog-js';
import { PostHogProvider } from 'posthog-js/react';
import { ReactNode } from 'react';

type PostHogProviderProps = {
    isEnabled: boolean;
    children: ReactNode;
    additionalOptions?: Partial<PostHogConfig>,
};

export function PostHogAnalyticsProvider({
    isEnabled,
    children,
    additionalOptions,
}: PostHogProviderProps) {
    return isEnabled ? (
        <PostHogProvider
            apiKey="phc_IggVMJtR5vawlA4H3IIYnIyWjcK8rPiqAI1FlmKZPjp"
            options={{
                // We'll set local storage as the default persistence method so
                // that we don't have to show cookie banners in our applications
                persistence: 'localStorage',
                // We need to manually collect page view events since
                // all of our applications use client-side routing
                capture_pageview: false,
                ...additionalOptions,
            }}
        >
            {children}
        </PostHogProvider>
    ) : (
        <>{children}</>
    );
}
