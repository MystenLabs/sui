# sui-move new example when example/tests exists should not generate any new example source but should otherwise
# operate normally

mkdir -p example/tests
echo "existing_ignore" >> example/.gitignore
# TODO: implement this functionality (https://linear.app/mysten-labs/issue/DVX-486/sui-move-new-will-clobber-existing-files)
#
# sui-move new example
# echo ==== project files ====
# ls example
# echo ==== sources ====
# ls example/sources
# echo ==== tests ====
# ls example/tests
# echo ==== .gitignore ====
# cat example/.gitignore
