# Sui Move by Example（中文版）

本书为[《Sui Move by Example》](https://examples.sui.io/)的中文版，尽管英文版已经足够通俗易懂，中文版本旨在为建设 Sui 中文社区贡献绵薄之力。

## 关于贡献

如果你愿意贡献你的一份力量，欢迎提交 pr 或 issue。

在翻译中将采取中英文对照以方便读者参考英文原版相对应的名词和语句，也方便后续翻译、更新。 如果需要查看英文内容点击【English Version】的折叠按钮即可。

！！！请注意书中代码示例中所有的中文注释仅为翻译需要，实际开发中 move 语言暂不支持 UTF-8 编码注释。

## 快速开始

和原版相同，本书将同样使用 [mdBook](https://rust-lang.github.io/mdBook/)

1. 安装 mdBook
```
    cargo install mdbook
```

2. 下载
```
    git clone https://github.com/MystenLabs/sui.git
```

3. 预览
```
    cd doc/book_zh && mdbook serve --open
```