import { Html, Head, Main, NextScript } from 'next/document';

export default function Document() {
    return (
        <Html>
            <Head>
                <link
                    rel="stylesheet"
                    href="https://cdn.jsdelivr.net/npm/picnic@7.1.0/picnic.min.css"
                    integrity="sha256-5T8QYQPUORrO1aSWJdN4bowilPiT+ot6fPfJperdmTU="
                    crossorigin="anonymous"
                />
            </Head>
            <body>
                <Main />
                <NextScript />
            </body>
        </Html>
    );
}
