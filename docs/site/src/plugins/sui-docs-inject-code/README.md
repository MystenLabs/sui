# `docusaurus-plugin-includes`

Plugin for Docusaurus to include the content of markdown in other markdown files.

Other than including files using [MDX transclusion](https://mdxjs.com/getting-started#documents), this plugin ensures that the content of the included markdown file also appears in the table of contents on the right side of the docusaurus page.

The plugin is intended to have shared content (e.g. chapters), that appears in multiple places of the same documentation, as well as shared content that is used in documentation of multiple websites.

In order that docusaurus can archive all used markdown files into `versioned_docs` folder when creating a new version, the included files also have to be located inside of the docs folder. So we need to copy the files shared with other products from outside into a subfolder of docs before we can use them. The plugin can also do this copy job for configured folders.

After build, the shared folders also appear in the build folder. The plugin can remove post build specific folders from build folder.

Because documentation blocks shared across multiple websites often contain project specific words like the project name, the plugin can also replace placeholders with project specific replacements specified in the docusaurus configuration file.

The plugin also supports placeholders in [remarkable-embed](https://www.npmjs.com/package/remarkable-embed) style `{@myPlugin: slug}` syntax, allowing you to embed rich content in your documents with your own JavaScript plugin function.

## Usage

### Compile, copy, install as local plugin

```
yarn add docusaurus-plugin-includes
```

or

```
npm install docusaurus-plugin-includes --save
```

Then adapt your `docusaurus.config.js` with the following block:

```
plugins: [
    [
      'docusaurus-plugin-includes',
      {
        sharedFolders: [
          { source: '../../_shared', target: '../docs/shared'},
        ],

        postBuildDeletedFolders: ['shared'],

        replacements: [
          { key: '{ProductName}', value: 'My long product name for XYZ' },
          { key: '{ShortName}', value: 'XYZ' },
        ],

        embeds: [
          {
            key: 'myAwesomePlugin',
            embedFunction: function(code) {
              return `...`;
            }
          }
        ],
        injectedHtmlTags: {
          preBodyTags: [`<link rel="stylesheet" href="https://cdn.example.com/style.css" type="text/css">`]
        }
      },
    ],
  ],
```

### Include markdown files in other markdown files

Add the following at the position where another file should be included:

```
{@include: pathRelativeToDocsFolder/markdownfile.md}
```

A real world sample is `{@include: shared/finesse_compatibility.md}`.
The path is relative from the main docs folder. In the sample above, the included file is located in a subfolder `shared` in the main docs folder.

The included markdown files must be plain markdown files **without** docusaurus headers with tags like `id`and `title`.

Included files are allowed to again include other files. Make sure to avoid endless include loops.

### Copy shared folder(s) from outside into docs folder

The shared files must also be located in the main `docs` folder to make sure they are also copied automatically from docusaurus into the `versioned_docs` folder when creating a version. Markdown files shared with other external product documentations must therefore somehow be copied from outside into a subfolder of the main docs folder.

The plugin adds 2 command line commands to automate copying these folder(s).

- `includes:copySharedFolders`: Copy the configured shared folders
- `includes:cleanCopySharedFolders`: Delete existing target folders first, copySharedFolders

The folders to copy can be configured in plugins configuration in `docusaurus.config.js` file.

```
  sharedFolders: [
    { source: '../../_shared', target: '../docs/shared'},
  ]
```

Source and target path are defined relative to the website folder where also the file `docusaurus.config.js` is located.

### Delete shared folders post build from build output directory

After docusaurus build, the shared folders also exist in the final build output directory, but we don't want that,

The plugin can also delete configured folders from all version subfolders of docusaurus build output.
Configure here the subfolder names (in `docs` folder of build output) that have to be deleted:

```
  postBuildDeletedFolders: ['shared']
```

### Replace placeholders

Because we often need the product name or the CRM name in the documentation, we need the possibility to add placeholders in the shared markdown files that will be replaced with the value for the current product.

The plugin also allows such placeholder replacements configured in `docusaurus.config.js` file.

```
  replacements: [
    { key: '{ProductName}', value: 'My Awesome Project' },
    { key: '{SolutionName}', value: 'My Awesome Solution' },
  ]
```

### Replace `remarkable-embed` style placeholders

The plugin also supports placeholders in [remarkable-embed](https://www.npmjs.com/package/remarkable-embed) style `{@myPlugin: slug}` syntax, allowing you to embed rich content in your documents with your own JavaScript plugin function.

You can configure such placeholder replacements in `docusaurus.config.js` file.

```
  embeds: [
    {
      key: 'myAwesomePlugin',
      embedFunction: function(code) {
        return `...`;
      }
    }
  ]
```

The following sample configuration adds plugin code to embed video files from assets folder with syntax `{@video: filename}` and youtube videos with syntax `{@youtube: videocode}`.

```
  embeds: [
    {
      key: 'video',
      embedFunction: function(code) {
        return `<video width="785" height="588" controls loop controlsList="nodownload">
                  <source type="video/mp4" src={require('./assets/${code}').default}></source>
                </video>`;
      }
    },
    {
      key: 'youtube',
      embedFunction: function(code) {
        return '<iframe width="785" height="440" type="text/html" frameborder="0" src="https://www.youtube.com/embed/' + code + '"></iframe>'
      }
    }
  ]
```

### Inject HTML Tags

Inject head and/or body HTML tags to Docusaurus generated HTML.

The following 3 identifiers can be used and they are all optional:

- headTags
- preBodyTags
- postBodyTags

See https://docusaurus.io/docs/lifecycle-apis#injecthtmltags for more information.

In the following example a CSS file gets loaded in the head tag and a h1 tag gets added to the body:

```
  injectedHtmlTags: {
    headTags: [`<link rel="stylesheet" href="https://cdn.example.com/style.css" type="text/css">`],
    preBodyTags: [`<h1>Test</h1>`]
  }
```
