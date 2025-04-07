// Keep track of files we renamed
let mut renamed_files = Vec::new();

// rename all .move files in the directory and subdirectories to .move.bak
// skip the file_path
for entry in glob::glob(&format!("{}/**/*.move", sources_dir.display())).expect("Failed to read glob pattern") {
    if let Ok(path) = entry {
        if path != *file_path {
            std::fs::rename(&path, path.with_extension("move.bak")).unwrap();
            renamed_files.push(path);
        }
    }
}

// Setup cleanup that will execute even in case of panic or early return 