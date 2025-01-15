# sui-move new example when example/Move.toml exists should fail and not touch any files

mkdir -p example/sources
echo "existing_ignore" >> example/.gitignore
echo "dummy toml" >> example/Move.toml

sui-move new example
echo ==== project files ====
ls example
echo ==== .gitignore ====
cat example/.gitignore
