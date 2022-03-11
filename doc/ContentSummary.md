# ADDING CONTENT TO DOCS SITE

## Basic

- All content for the documentation site are Markdown files in the [src/](src/) folder.
- Markdown content should follow basic [Markdown syntax](https://www.markdownguide.org/basic-syntax/).
- The name of the Markdown file make up the end of the page URL route and should include the page's subject and be lowercase. File names with multiple words should be separated with a dash. For example: `getting-started.md` in learn folder corresponds to the `.../learn/getting-started/` file path.
- Every new Markdown file should contain YAML-like front matter at the top. It is the first thing in the file and must take the form of valid YAML set between triple-dashed lines. See [Front Matter](#front-matter) for a basic example.
- Folders in [src/](src/) represent categories, and Markdown files in that folder should be intuitively named.
- Use [Headings](#Headings) to create new sections that automatically generate menu links in the `table of contents` at the top; see [Table of Contents](#table-of-content).

## Adding content

The workflow for adding content to the Docs site is as follows:

- Content should be created in [content/docs](https://github.com/MystenLabs/fastx_dev_portal/tree/content/content/docs) on the `content` branch of the GitHub repo.
- Click `Add file` or `Upload files` on the GitHub page.
- Changes committed to the `content` branch can be view on [QA site](https://devportal-qa.web.app/). It takes a few minutes for the changes to propagate to the QA site.
- Once complete, make a pull to the main branch.
- The content will be added to the site automatically once merged with the `main` repo.

## Front Matter

This is required for every Markdown file.

- `title`: The page title, meta tag, navigation menu name, and breadcrumb name.

```
---
title: Storage
---
```

Example:
Markdown file content example:

See [YAML Front Matter](https://www.markdownguide.org/yaml-front-matter/) for more information.

## Headings

```
  # Lorem ipsum
  ## dolor—sit—amet
  ### consectetur &amp; adipisicing
  #### elit
  ##### elit
```

## Table of Content

[![Table Of Content image](/static/MD-assets/tableOfcontent.jpg 'Table Of Content image')](/MD-assets/tableOfcontent.jpg)

## Codeblocks

Codeblocks in Markdown are wrapped inside three backticks. Optionally, you can define the language of the codeblock to enable specific syntax highlighting.

- Highlighted line numbers inside curly braces
- Filename inside square brackets

```js{1,3-5}[server.js]
const http = require('http')
const bodyParser = require('body-parser')

http.createServer((req, res) => {
  bodyParser.parse(req, (error, body) => {
    res.end(body)
  })
}).listen(3000)
```
