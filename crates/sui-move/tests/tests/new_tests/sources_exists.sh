# sui-move new example when example/sources exists should not generate any new example source but should otherwise
# operate normally

# TODO: implement this functionality (https://linear.app/mysten-labs/issue/DVX-486/sui-move-new-will-clobber-existing-files)
# mkdir -p example/sources
# echo "existing_ignore" >> example/.gitignore
#
# sui-move new example
# echo ==== project files ====
# ls example
# echo ==== sources ====
# ls example/sources
#
# echo ==== .gitignore ====
# cat example/.gitignore
