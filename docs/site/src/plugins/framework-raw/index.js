// plugins/framework-raw/index.js
const fs = require('fs');
const path = require('path');

function walkFiles(rootDir) {
  const out = [];
  (function walk(dir) {
    for (const name of fs.readdirSync(dir)) {
      const abs = path.join(dir, name);
      const st = fs.statSync(abs);
      if (st.isDirectory()) walk(abs);
      else out.push(abs);
    }
  })(rootDir);
  return out;
}

function ensureDir(p) {
  fs.mkdirSync(p, { recursive: true });
}

function copyFile(src, dest) {
  ensureDir(path.dirname(dest));
  fs.copyFileSync(src, dest);
}

function relWithoutExt(root, abs) {
  const rel = path.relative(root, abs).replace(/\\/g, '/');
  return rel.replace(/\.(md|mdx|markdown)$/i, '');
}

function computeRouteBase(relNoExt) {
  // Keep the same relative structure under /references/framework
  return `/references/framework/${relNoExt}`;
}

module.exports = function frameworkRawPlugin(context, options) {
  const siteDir = context.siteDir;

  // Where your original files live (adjust if needed)
  const srcRootA = path.resolve(siteDir, 'docs', 'references', 'framework');
  const srcRootB = path.resolve(siteDir, 'docs', 'content', 'references', 'framework'); // optional alt
  const srcRoots = [srcRootA, srcRootB].filter((p) => fs.existsSync(p));

  // Where we mirror them as static assets
  const staticRoot = path.resolve(siteDir, 'static', '_framework');

  return {
    name: 'framework-raw-plugin',

    getPathsToWatch() {
      // Let dev server watch for changes
      const globs = [];
      for (const root of srcRoots) {
        globs.push(`${root}/**/*`);
      }
      return globs;
    },

    async loadContent() {
      // Build a file index for routes
      const index = [];
      for (const root of srcRoots) {
        const files = walkFiles(root);
        for (const abs of files) {
          index.push({ root, abs });
        }
      }
      return { index };
    },

    async contentLoaded({ content, actions }) {
      const { createData, addRoute } = actions;
      for (const { root, abs } of content.index) {
        const rel = path.relative(root, abs).replace(/\\/g, '/');
        const relNoExt = relWithoutExt(root, abs);

        // Copy to static/_framework/<rel>
        const dest = path.join(staticRoot, rel);
        ensureDir(path.dirname(dest));
        copyFile(abs, dest);

        // Put per-file metadata into the build data dir
        const dataPath = await createData(
          `framework/${relNoExt.replace(/[\\/]/g, '__')}.json`,
          JSON.stringify({ rel }), // only need rel
        );

        // Create a route that uses our viewer component
        addRoute({
          path: computeRouteBase(relNoExt),
          exact: true,
          component: '@site/src/components/FrameworkViewer.tsx',
          modules: {
            meta: dataPath,
          },
        });
      }
    },
  };
};
