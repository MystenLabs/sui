# basic test that sui-move new outputs correct files
sui-move new example
echo ==== files in project ====
ls -A example
echo ==== files in sources ====
ls -A example/sources
echo ==== files in tests =====
ls -A example/tests
