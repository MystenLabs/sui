const { createProxyMiddleware } = require('http-proxy-middleware');

const apiUrl = process.env.PROXY_API_SERVER || 'http://localhost:8080';

module.exports = function (app) {
    app.use(
        '/api',
        createProxyMiddleware({
            target: apiUrl,
            changeOrigin: true,
            pathRewrite: {
                '^/api': '/',
            },
        })
    );
};
