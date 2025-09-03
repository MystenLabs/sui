This folder contains the scripts to run `sui move build` on a number of Move projects. 

Required folder structure
- projects
- results
- scripts

In the `projects` folder, there should be folders with Move packages. The actual Move package can be nested inside another folder. The script will pick all the folders with a .toml file.

The results folder will contain logs for every single project.

The scripts folder contains one or more scripts
- build_all_projects_parallel.sh -- this will run `sui move build` on all projects in ../projects. It uses 16 threads by default, so if you want to change it you'd need to set it in the script at the top.


## How to run this

You will need first to get the dataset from here: https://github.com/CMU-SuiGPT/sui-move-dataset-v1
Some massaging might be needed: check out the submodules, delete the `.git` files once the submodules are checked out (not the .git folder, but just the .git file). Some projects have local dependencies to `sui` framework in various forms
`../sui` or `../../../crates/sui-framework` or `../../sui-framework`, etc. All these need to be fixed before running the scripts.
