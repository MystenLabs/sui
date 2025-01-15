# sui-move new example when example/sources exists should not generate any new example source but should otherwise
# operate normally

mkdir -p example/sources
echo "existing_ignore" >> example/.gitignore

sui-move new example
echo ==== project files ====
ls example
echo ==== sources ====
ls example/sources

echo ==== .gitignore ====
cat example/.gitignore
