// src/components/FrameworkViewer.tsx
import React from 'react';
import Layout from '@theme/Layout';
import BrowserOnly from '@docusaurus/BrowserOnly';
import useBaseUrl from '@docusaurus/useBaseUrl';

type Meta = { rel: string };
type Props = { meta: Meta };

function escapeHtml(s: string) {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

function Inner({ rel }: { rel: string }) {
  const [text, setText] = React.useState<string>('Loading…');
  const url = useBaseUrl(`/_framework/${rel}`);

  React.useEffect(() => {
    let cancelled = false;
    fetch(url, { cache: 'no-store' })
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(String(r.status)))))
      .then((t) => !cancelled && setText(t))
      .catch((e) => !cancelled && setText(`Failed to load: ${String(e)}`));
    return () => {
      cancelled = true;
    };
  }, [url]);

  // IMPORTANT: no "language-*" class -> Prism won't touch it.
  return (
    <div style={{ maxWidth: '100%', overflowX: 'auto' }}>
      <pre style={{ margin: 0 }}>
        <code dangerouslySetInnerHTML={{ __html: escapeHtml(text) }} />
      </pre>
    </div>
  );
}

export default function FrameworkViewer({ meta }: Props) {
  const rel = meta?.rel ?? '';
  return (
    <Layout title={rel} description={rel}>
      <main style={{ padding: '1rem 0' }}>
        <BrowserOnly fallback={<div>Loading…</div>}>
          {() => <Inner rel={rel} />}
        </BrowserOnly>
      </main>
    </Layout>
  );
}
