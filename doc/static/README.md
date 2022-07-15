# Markdown Assets

**This directory is required. All assets, such as PDFs and images, should be stored in this directory**

Reference this directory in your Markdown file, like so:

```

![Table Of Content image](../../static/tableOfcontent.jpg "Table Of Content image")]
```

## Best practice for frequently changed assets
 - upon each release, create a sub-directory, for example `wallet_0.0.3`, upload updated assets following the same naming conventions
 - search and replace references of previous version with new asset path, like `wallet_0.0.2/<name>` to `wallet_0.0.3/name`
 - in case of new assets, contact Clay Murphy to add to corresponding markdown files accordingly.
