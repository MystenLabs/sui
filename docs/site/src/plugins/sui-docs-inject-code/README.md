# `sui-docs-inject-code`

Plugin for Docusaurus to include the content of code in markdown files.

This plugin mostly just modifies `docusaurus-plugin-includes` to include code source instead of other markdown files.

## Usage


### Include code source in markdown files

Add the following at the position where code source should be included:

```
{@inject: pathRelativeToSuiRepo/codefile.ext}
```

### Include only a section of code

To include a specific section of code only, include ID opening and closing markers in code comments using the following format:

```
// docs::#idname
... code that's included
// docs::/#idname
```

Use the ID in the markdown calling the code:

```
{@inject: pathRelativeToSuiRepo/codefile.ext#idname}
```

### Add closing syntax to code

Sometimes when calling only a section of code, you need to close code blocks. Rather than using multiple id comments, you can add them to the closing:

```
// docs::#idname
...code that's included
// docs::#idname);}
```

This appends the following to the code section in the docs:

```
... code source between #idname
  );
}
```