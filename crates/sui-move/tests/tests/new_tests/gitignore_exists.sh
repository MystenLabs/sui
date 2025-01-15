# check that sui-move new correctly updates existing .gitignore
mkdir example
echo "existing_ignore" > example/.gitignore
sui-move new example
cat example/.gitignore
