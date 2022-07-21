# Markdown Assets

**This directory is required. All assets, such as PDFs and images, should be stored in this directory**

Reference this directory in your Markdown file, like so:

```
![Table Of Content image](../../static/tableOfcontent.jpg "Table Of Content image")]
```

Going forward, we will start to group images by topic in subdirectories, for example:

```
static/wallet
static/explorer
```

For frequently changed assets where multiple versions of the product are supported at once, you may create version-specific sub-directories, for example `wallet_0.0.3`. Then search and replace references to the previous version with new asset path, such as from `static/wallet/0.0.2/image.png` to `static/wallet/0.0.3/image.png`

For new assets, work with Clay-Mysten and Randall-Mysten to include them in corresponding Markdown files.
