import { createSitemapRoutes } from './utils/createSitemapRoutes'
import { menuOrderGenerator } from './utils/menuFilter'
import { siteConfig } from './site.config'

export default {
  // Target: https://go.nuxtjs.dev/config-target
  target: 'static',
  // mode: 'universal',
  // Global page headers: https://go.nuxtjs.dev/config-head
  head: {
    title: siteConfig.title,

    meta: [
      { charset: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { hid: 'description', name: 'description', content: '' },
      { name: 'format-detection', content: 'telephone=no' },
    ],
    link: [{ rel: 'icon', type: 'image/x-icon', href: siteConfig.favIcon }],
  },

  // Global CSS: https://go.nuxtjs.dev/config-css
  css: ['~/assets/styles/css/vendor.css', '~/assets/styles/scss/app.scss'],

  // env config
  env: {
    baseUrl: process.env.BASE_URL || 'http://localhost:3000',
  },

  // Plugins to run before rendering page: https://go.nuxtjs.dev/config-plugins
  plugins: [
    {
      src: '~/plugins/core',
      ssr: false,
      mode: 'client',
    },
    {
      src: '~/plugins/scrollpy',
      ssr: false,
      mode: 'client',
    },
    {
      src: '~/plugins/analytics',
      ssr: false,
      mode: 'client',
    },
  ],

  // localhost server
  server: {
    // port: 5050, // default: 3000 // 5050
    default: 3000,
    host: '0.0.0.0', // default: localhost
  },

  // parent ID name
  globalName: 'sui',

  // Auto import components: https://go.nuxtjs.dev/config-components
  components: true,
  router: {
    linkActiveClass: '_selectActive',
    trailingSlash: true,
    linkExactActiveClass: 'exact-active',
    async scrollBehavior(to, from, savedPosition) {
      if (savedPosition) {
        return savedPosition
      }
      if (to.hash) {
        return { el: to.hash, behavior: 'smooth' }
      }
      return { x: 0, y: 0 }
    },
  },
  // Static generation: https://go.nuxtjs.dev/config-generate
  generate: {
    dir: './public',
    subFolders: true,
    exclude: [],
    fallback: '404.html',
  },

  // Modules for dev and build (recommended): https://go.nuxtjs.dev/config-modules
  buildModules: [
    // https://go.nuxtjs.dev/typescript
    '@nuxt/typescript-build',
  ],

  // build folder
  buildDir: 'suibuilds',

  // Modules: https://go.nuxtjs.dev/config-modules
  modules: [
    // https://go.nuxtjs.dev/content
    '@nuxt/content',

    // sitemap generator
    '@nuxtjs/sitemap',
  ],
  hooks: {
    'content:file:beforeParse': (file) => {
      if (file.extension !== '.md') return

      // Add Custom class to markdown tables
      // replace .md links with .html links
      //href="move.md"
      const tableClosing = "</table></div>'"

      file.data = file.data
        .replace(/<table>/g, '<div class="suiTable"><table>')
        .replace(/<\/table>/g, tableClosing)
        // .replace(/.md/g, '/')
    },
    'content:file:beforeInsert': (document) => {
      if (document.extension === '.md') {
        // Use file name when the title field in front-matter is not defined
        let title = document.title || document.slug

        const resp = menuOrderGenerator(title, document.path)
        document.title = title
        if (resp && resp.menuItemIndex) {
          document.categoryOrder = resp.categoryIndex
          document.itemOder = resp.menuItemIndex | 0
          document.categoryName = resp.categoryName

          /// add subCategoryName to the document when it is a subCategory
          if (resp.subCategoryName) {
            document.subCategoryName = resp.subCategoryName
          }
        }
      }
    },
  },
  /// Generate sitemap.xml base on the domain name
  sitemap: {
    hostname: 'https://devportal-30dd0.web.app',
    gzip: true,
    routes: createSitemapRoutes,
  },

  // Content module configuration: https://go.nuxtjs.dev/config-content
  content: {
    // read content from src directory /
    dir: 'src',
    fullTextSearchFields: ['title', 'description'],
    liveEdit: false,
    markdown: {
      // remarkPlugins: ['remark-footnotes', 'remark-mermaidjs'],
      prism: {
        theme: 'prism-themes/themes/prism-one-light.css',
      },
      //
    },
    // nestedProperties: ['docs.slug']
  },

  // Build Configuration: https://go.nuxtjs.dev/config-build
  build: {
    extractCSS: true,
  },
}
