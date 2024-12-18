// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#[allow(unused_imports)]
use log::{debug, info, warn};

use crate::code_writer::{CodeWriter, CodeWriterLabel};
use itertools::Itertools;
use move_binary_format::file_format;
use move_compiler::{
    diagnostics::ByteSpan,
    expansion::ast::{TargetKind, Visibility},
    parser::keywords::{BUILTINS, CONTEXTUAL_KEYWORDS, KEYWORDS},
};
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::Loc;
use move_model_2::{
    display as model_display,
    source_model::{self as model, Model},
    ModuleId, QualifiedMemberId,
};
use move_symbol_pool::Symbol;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fmt::Write as FmtWrite,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// The maximum number of subheadings that are allowed
const MAX_SUBSECTIONS: usize = 6;

/// Options passed into the documentation generator.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DocgenOptions {
    /// The level where we start sectioning. Often markdown sections are rendered with
    /// unnecessary large section fonts, setting this value high reduces the size.
    pub section_level_start: usize,
    /// Whether to include private functions in the generated docs.
    pub include_private_fun: bool,
    /// Whether to include specifications in the generated docs.
    pub include_specs: bool,
    /// Whether to put specifications in the same section as a declaration or put them all
    /// into an independent section.
    pub specs_inlined: bool,
    /// Whether to include Move implementations.
    pub include_impl: bool,
    /// Max depth to which sections are displayed in table-of-contents.
    pub toc_depth: usize,
    /// Whether to use collapsed sections (<details>) for impl and specs
    pub collapsed_sections: bool,
    /// In which directory to store output.
    pub output_directory: String,
    /// In which directories to look for references.
    pub doc_path: Vec<String>,
    /// A list of paths to files containing templates for root documents for the generated
    /// documentation.
    ///
    /// A root document is a markdown file which contains placeholders for generated
    /// documentation content. It is also processed following the same rules than
    /// documentation comments in Move, including creation of cross-references and
    /// Move code highlighting.
    ///
    /// A placeholder is a single line starting with a markdown quotation marker
    /// of the following form:
    ///
    /// ```notrust
    /// > {{move-include NAME_OF_MODULE_OR_SCRIPT}}
    /// > {{move-toc}}
    /// > {{move-index}}
    /// ```
    ///
    /// These lines will be replaced by the generated content of the module or script,
    /// or a table of contents, respectively.
    ///
    /// For a module or script which is included in the root document, no
    /// separate file is generated. References between the included and the standalone
    /// module/script content work transparently.
    pub root_doc_templates: Vec<String>,
    /// An optional file containing reference definitions. The content of this file will
    /// be added to each generated markdown doc.
    pub references_file: Option<String>,
    /// Whether to include dependency diagrams in the generated docs.
    pub include_dep_diagrams: bool,
    /// Whether to include call diagrams in the generated docs.
    pub include_call_diagrams: bool,
    /// If this is being compiled relative to a different place where it will be stored (output directory).
    pub compile_relative_to_output_dir: bool,
}

impl Default for DocgenOptions {
    fn default() -> Self {
        Self {
            section_level_start: 1,
            include_private_fun: true,
            include_specs: true,
            specs_inlined: true,
            include_impl: true,
            toc_depth: 3,
            collapsed_sections: true,
            output_directory: "doc".to_string(),
            doc_path: vec!["doc".to_string()],
            compile_relative_to_output_dir: false,
            root_doc_templates: vec![],
            references_file: None,
            include_dep_diagrams: false,
            include_call_diagrams: false,
        }
    }
}

/// The documentation generator.
pub struct Docgen<'env> {
    options: &'env DocgenOptions,
    /// preferred modules to be used in the generated documentation.
    preferred_modules: BTreeMap<Symbol, AccountAddress>,
    /// A list of file names and output generated for those files.
    output: Vec<(String, String)>,
    /// Map from module id to information about this module.
    infos: BTreeMap<ModuleId, ModuleInfo>,
    /// Current code writer.
    writer: CodeWriter,
    current_module: Option<ModuleId>,
    /// A counter for labels.
    label_counter: usize,
    /// A table-of-contents list.
    toc: Vec<(usize, TocEntry)>,
    /// The current section next
    section_nest: usize,
    /// The last user provided (via an explicit # header) section nest.
    last_root_section_nest: usize,
    errors: Vec<String>,
}

/// Information about the generated documentation for a specific script or module.
#[derive(Debug, Default, Clone)]
struct ModuleInfo {
    /// The file in which the generated content for this module is located. This has a path
    /// relative to the `options.output_directory`.
    target_file: String,
    /// The label in this file.
    label: String,
    /// Whether this module is included in another document instead of living in its own file.
    /// Among others, we do not generate table-of-contents for included modules.
    is_included: bool,
}

/// A table-of-contents entry.
#[derive(Debug, Default, Clone)]
struct TocEntry {
    label: String,
    title: String,
}

/// An element of the parsed root document template.
enum TemplateElement {
    Text(String),
    IncludeModule(Symbol),
    IncludeToc,
    Index,
}

impl<'env> Docgen<'env> {
    /// Creates a new documentation generator.
    pub fn new(env: &Model, root_package: Symbol, options: &'env DocgenOptions) -> Self {
        let preferred_modules = env
            .modules()
            .filter(|m| m.info().package.is_some_and(|p| p == root_package))
            .map(|m| {
                let (a, n) = m.id();
                (n, a)
            })
            .collect();
        Self {
            preferred_modules,
            options,
            output: Default::default(),
            infos: Default::default(),
            writer: CodeWriter::new(),
            label_counter: 0,
            current_module: None,
            toc: vec![],
            section_nest: 0,
            last_root_section_nest: 0,
            errors: vec![],
        }
    }

    /// Generate document contents, returning pairs of output file names and generated contents.
    pub fn gen(mut self, env: &Model) -> Vec<(String, String)> {
        // If there is a root templates, parse them.
        let root_templates = self
            .options
            .root_doc_templates
            .iter()
            .filter_map(|file_name| {
                let root_out_name = PathBuf::from(file_name)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace("_template", "");
                match self.parse_root_template(file_name) {
                    Ok(elements) => Some((root_out_name, elements)),
                    Err(_) => {
                        self.unknown_loc_error(format!(
                            "cannot read root template `{}`",
                            file_name
                        ));
                        None
                    }
                }
            })
            .collect_vec();

        // Compute module infos.
        self.compute_module_infos(env, &root_templates);

        // Expand all root templates.
        for (out_file, elements) in root_templates {
            self.expand_root_template(env, &out_file, elements);
        }

        // Generate documentation for standalone modules which are not included in the templates.
        let ids_not_included = self
            .infos
            .iter()
            .filter_map(|(id, info)| if info.is_included { None } else { Some(*id) })
            .collect::<Vec<_>>();
        for id in ids_not_included {
            self.gen_module(env, id);
            let info = self.infos.get(&id).unwrap();
            let path = self.make_file_in_out_dir(&info.target_file);
            self.output.push((path, self.writer.extract_result()));
        }

        // If there is a references_file, append it's content to each generated output.
        if let Some(fname) = &self.options.references_file {
            let mut content = String::new();
            if File::open(fname)
                .and_then(|mut file| file.read_to_string(&mut content))
                .is_ok()
            {
                let trimmed_content = content.trim();
                if !trimmed_content.is_empty() {
                    for (_, out) in self.output.iter_mut() {
                        out.push_str("\n\n");
                        out.push_str(trimmed_content);
                        out.push('\n');
                    }
                }
            } else {
                self.unknown_loc_error(format!("cannot read references file `{fname}`"));
            }
        }

        self.output
    }

    fn unknown_loc_error(&mut self, msg: impl ToString) {
        self.errors.push(msg.to_string());
    }

    /// Parse a root template.
    fn parse_root_template(&mut self, file_name: &str) -> anyhow::Result<Vec<TemplateElement>> {
        static REX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
                r"(?xm)^\s*>\s*\{\{
                ( (?P<include>move-include\s+(?P<include_name>\w+))
                | (?P<toc>move-toc)
                | (?P<index>move-index)
                )\s*}}.*$",
            )
            .unwrap()
        });
        let mut content = String::new();
        let mut file = File::open(file_name)?;
        file.read_to_string(&mut content)?;
        let mut at = 0;
        let mut res = vec![];
        while let Some(cap) = REX.captures(&content[at..]) {
            let start = cap.get(0).unwrap().start();
            let end = cap.get(0).unwrap().end();
            if start > 0 {
                res.push(TemplateElement::Text(content[at..at + start].to_string()));
            }
            if cap.name("include").is_some() {
                let name = cap.name("include_name").unwrap().as_str();
                res.push(TemplateElement::IncludeModule(name.into()));
            } else if cap.name("toc").is_some() {
                res.push(TemplateElement::IncludeToc);
            } else if cap.name("index").is_some() {
                res.push(TemplateElement::Index);
            } else {
                unreachable!("regex misbehavior");
            }
            at += end;
        }
        if at < content.len() {
            res.push(TemplateElement::Text(content[at..].to_string()));
        }
        Ok(res)
    }

    /// Expand the root template.
    fn expand_root_template(
        &mut self,
        env: &Model,
        output_file_name: &str,
        elements: Vec<TemplateElement>,
    ) {
        self.writer = CodeWriter::new();
        self.label_counter = 0;
        let mut toc_label = None;
        self.toc = vec![];
        for elem in elements {
            assert!(self.writer.output_is_empty());
            match elem {
                TemplateElement::Text(str) => self.doc_text_for_root(env, &str),
                TemplateElement::IncludeModule(name) => {
                    let Some(addr) = self.preferred_modules.get(&name) else {
                        writeln!(self.writer, "> undefined move-include `{name}`").unwrap();
                        continue;
                    };
                    let id = (*addr, name);
                    let info = self.infos.get(&id).expect("module defined");

                    assert!(info.is_included);
                    // Generate the module content in place, adjusting the section nest to
                    // the last user provided one. This will nest the module underneath
                    // whatever section is in the template.
                    let saved_nest = self.section_nest;
                    self.section_nest = self.last_root_section_nest + 1;
                    self.gen_module(env, id);
                    self.section_nest = saved_nest;
                }
                TemplateElement::IncludeToc => {
                    if toc_label.is_none() {
                        toc_label = Some(self.writer.create_label());
                    } else {
                        // CodeWriter can only maintain one label at a time.
                        writeln!(self.writer, ">> duplicate move-toc (technical restriction)")
                            .unwrap();
                    }
                }
                TemplateElement::Index => {
                    self.gen_index(env);
                }
            }
        }
        if let Some(label) = toc_label {
            // Insert the TOC.
            self.gen_toc(label);
        }

        // Add result to output.
        self.output.push((
            self.make_file_in_out_dir(output_file_name),
            self.writer.extract_result(),
        ));
    }

    /// Compute ModuleInfo for all modules, considering root template content.
    fn compute_module_infos(&mut self, env: &Model, templates: &[(String, Vec<TemplateElement>)]) {
        let mut out_dir = self.options.output_directory.to_string();
        if out_dir.is_empty() {
            out_dir = ".".to_string();
        }
        let log = |m: model::Module<'_>, i: &ModuleInfo| {
            info!(
                "Module `{}` in file `{}/{}` {}",
                m.ident(),
                out_dir,
                i.target_file,
                if matches!(
                    m.info().target_kind,
                    TargetKind::Source {
                        is_root_package: true
                    }
                ) {
                    "is target"
                } else {
                    "is a dependency"
                }
            );
        };
        // First process infos for modules included via template.
        let mut included = BTreeSet::new();
        for (template_out_file, elements) in templates {
            for element in elements {
                if let TemplateElement::IncludeModule(name) = element {
                    // TODO: currently we only support simple names, we may want to add support for
                    //   address qualification.
                    let id = (self.preferred_modules[name], *name);
                    if let Some(module_env) = env.maybe_module(id) {
                        let info = ModuleInfo {
                            target_file: template_out_file.to_string(),
                            label: self.make_label_for_module(module_env),
                            is_included: true,
                        };
                        log(module_env, &info);
                        self.infos.insert(id, info);
                        included.insert(id);
                    } else {
                        // If this is not defined, we continue anyway and will not expand
                        // the placeholder in the generated root doc (following common template
                        // practice).
                    }
                }
            }
        }
        // Now process infos for all remaining modules.
        for m in env.modules() {
            let id = m.id();
            if !included.contains(&id) {
                if let Some(file_name) = self.compute_output_file(m) {
                    let info = ModuleInfo {
                        target_file: file_name,
                        label: self.make_label_for_module(m),
                        is_included: false,
                    };
                    log(m, &info);
                    self.infos.insert(id, info);
                }
            }
        }
    }

    /// Computes file location for a module. This considers if the module is a dependency
    /// and if so attempts to locate already generated documentation for it.
    fn compute_output_file(&self, module_env: model::Module<'_>) -> Option<String> {
        let output_path = PathBuf::from(&self.options.output_directory);
        let file_name = PathBuf::from(module_env.source_path().as_str())
            .with_extension("md")
            .file_name()
            .expect("file name")
            .to_os_string();
        if !matches!(module_env.info().target_kind, TargetKind::External) {
            // Try to locate the file in the provided search path.
            self.options.doc_path.iter().find_map(|dir| {
                let mut path = PathBuf::from(dir);
                path.push(&file_name);
                if path.exists() {
                    Some(
                        self.path_relative_to(&path, &output_path)
                            .to_string_lossy()
                            .to_string(),
                    )
                } else {
                    // If it's a dependency traverse back up to finde the package name so that we
                    // can generate the documentation in the right place.
                    let path = PathBuf::from(module_env.source_path().as_str());
                    let package_name = path.ancestors().find_map(|dir| {
                        let mut path = PathBuf::from(dir);
                        path.push("Move.toml");
                        if path.exists() {
                            dir.file_stem()
                        } else {
                            None
                        }
                    });
                    package_name.map(|package_name| {
                        format!(
                            "dependencies/{}/{}",
                            package_name.to_string_lossy(),
                            file_name.to_string_lossy()
                        )
                    })
                }
            })
        } else {
            // We will generate this file in the provided output directory.
            Some(file_name.to_string_lossy().to_string())
        }
    }

    /// Make a file name in the output directory.
    fn make_file_in_out_dir(&self, name: &str) -> String {
        if self.options.compile_relative_to_output_dir {
            name.to_string()
        } else {
            let mut path = PathBuf::from(&self.options.output_directory);
            path.push(name);
            path.to_string_lossy().to_string()
        }
    }

    /// Make path relative to other path.
    fn path_relative_to(&self, path: &Path, to: &Path) -> PathBuf {
        if path.is_absolute() || to.is_absolute() {
            path.to_path_buf()
        } else {
            let mut result = PathBuf::new();
            for _ in to.components() {
                result.push("..");
            }
            result.join(path)
        }
    }

    /// Generates documentation for a module. The result is written into the current code
    /// writer. Writer and other state is initialized if this module is standalone.
    fn gen_module(&mut self, env: &Model, id: ModuleId) {
        let info = self.infos.get(&id).unwrap();
        let info_is_included = info.is_included;
        if !info_is_included {
            // (Re-) initialize state for this module.
            self.writer = CodeWriter::new();
            self.toc = vec![];
            self.section_nest = 0;
            self.label_counter = 0;
        }
        self.current_module = Some(id);

        // Print header
        let module_env = env.module(id);
        let module_name = module_env.ident();
        let label = info.label.clone();
        self.section_header(&format!("Module `{}`", module_name), &label);

        self.increment_section_nest();

        // Document module overview.
        self.doc_text(env, module_env.doc());

        // If this is a standalone doc, generate TOC header.
        let toc_label = if info_is_included {
            Some(self.gen_toc_header())
        } else {
            None
        };

        // Generate usage information.
        // We currently only include modules used in bytecode -- including specs
        // creates a large usage list because of schema inclusion quickly pulling in
        // many modules.
        self.begin_code();
        let used_modules = module_env
            .deps()
            .keys()
            .map(|id| format!("{}", env.module(*id).ident()))
            .sorted();
        for used_module in used_modules {
            self.code_text(env, &format!("use {};", used_module));
        }
        self.end_code();

        if self.options.include_dep_diagrams {
            self.gen_dependency_diagram(env, id, true);
            self.begin_collapsed(&format!(
                "Show all the modules that \"{}\" depends on directly or indirectly",
                module_env.ident()
            ));
            self.image(&format!("img/{}_forward_dep.svg", module_name));
            self.end_collapsed();

            self.gen_dependency_diagram(env, id, false);
            self.begin_collapsed(&format!(
                "Show all the modules that depend on \"{}\" directly or indirectly",
                module_name
            ));
            self.image(&format!("img/{}_backward_dep.svg", module_name));
            self.end_collapsed();
        }

        for s in module_env.structs().sorted_by_key(|s| s.compiled_idx()) {
            self.gen_struct(s);
        }

        if !module_env.enums().next().is_some() {
            for s in module_env
                .enums()
                .sorted_by(|a, b| Ord::cmp(&a.get_loc(), &b.get_loc()))
            {
                self.gen_enum(&s);
            }
        }

        if module_env.constants().next().is_some() {
            // Introduce a Constant section
            self.gen_named_constants(env);
        }

        let funs = module_env
            .functions()
            .filter(|f| {
                self.options.include_private_fun || {
                    let info = f.info();
                    info.entry.is_some() || !matches!(info.visibility, Visibility::Public(_))
                }
            })
            .sorted_by_key(|f| f.compiled_idx())
            .collect_vec();
        if !funs.is_empty() {
            for f in funs {
                self.gen_function(f);
            }
        }

        self.decrement_section_nest();

        // Generate table of contents if this is standalone.
        if let Some(label) = toc_label {
            self.gen_toc(label);
        }
    }

    /// Generate a static call diagram (.svg) starting from the given function.
    fn gen_call_diagram(
        &mut self,
        env: &Model,
        module: ModuleId,
        function: Symbol,
        is_forward: bool,
    ) {
        let module_env = env.module(module);
        let fun_env = module_env.function(function);
        let name_of = |other: model::Function<'_>| {
            if fun_env.module().id() == other.module().id() {
                other.name().to_string()
            } else {
                let other_env = env.module(other.module().id());
                format!("\"{}::{}\"", other_env.ident(), other.name())
            }
        };

        let mut dot_src_lines: Vec<String> = vec!["digraph G {".to_string()];
        let mut visited: BTreeSet<QualifiedMemberId> = BTreeSet::new();
        let mut queue: VecDeque<QualifiedMemberId> = VecDeque::new();

        let fun_id = (module, function);
        visited.insert(fun_id);
        queue.push_back(fun_id);

        while let Some((mid, fname)) = queue.pop_front() {
            let curr_env = env.module(mid).function(fname);
            let curr_name = name_of(curr_env);
            let next_list = if is_forward {
                curr_env.calls()
            } else {
                curr_env.called_by()
            };

            if fun_env.module().id() == curr_env.module().id() {
                dot_src_lines.push(format!("\t{}", curr_name));
            } else {
                let module_ident = env.module(curr_env.module().id()).ident();
                dot_src_lines.push(format!("\tsubgraph cluster_{} {{", module_ident));
                dot_src_lines.push(format!("\t\tlabel = \"{}\";", module_ident));
                dot_src_lines.push(format!("\t\t{}[label=\"{}\"]", curr_name, curr_env.name()));
                dot_src_lines.push("\t}".to_string());
            }

            for next_id in next_list.iter() {
                let next_env = env.module(next_id.0).function(next_id.1);
                let next_name = name_of(next_env);
                if is_forward {
                    dot_src_lines.push(format!("\t{} -> {}", curr_name, next_name));
                } else {
                    dot_src_lines.push(format!("\t{} -> {}", next_name, curr_name));
                }
                if !visited.contains(next_id) {
                    visited.insert(*next_id);
                    queue.push_back(*next_id);
                }
            }
        }
        dot_src_lines.push("}".to_string());

        let full_name = format!("{}::{}", module_env.ident(), fun_env.name());
        let out_file_path = PathBuf::from(&self.options.output_directory)
            .join("img")
            .join(format!(
                "{}_{}_call_graph.svg",
                full_name.replace("::", "_"),
                if is_forward { "forward" } else { "backward" }
            ));

        self.gen_svg_file(&out_file_path, &dot_src_lines.join("\n"));
    }

    /// Generate a forward (or backward) dependency diagram (.svg) for the given module.
    fn gen_dependency_diagram(&mut self, env: &Model, module_id: ModuleId, is_forward: bool) {
        let module_env = env.module(module_id);
        let module_name = module_env.ident();

        let mut dot_src_lines: Vec<String> = vec!["digraph G {".to_string()];
        let mut visited: BTreeSet<ModuleId> = BTreeSet::new();
        let mut queue: VecDeque<ModuleId> = VecDeque::new();

        visited.insert(module_id);
        queue.push_back(module_id);

        while let Some(id) = queue.pop_front() {
            let mod_env = env.module(id);
            let mod_name = mod_env.ident();
            let dep_list = if is_forward {
                mod_env.deps()
            } else {
                mod_env.used_by()
            };
            dot_src_lines.push(format!("\t{}", mod_name));
            for dep_id in dep_list.keys() {
                let dep_env = env.module(dep_id);
                let dep_name = dep_env.ident();
                if is_forward {
                    dot_src_lines.push(format!("\t{} -> {}", mod_name, dep_name));
                } else {
                    dot_src_lines.push(format!("\t{} -> {}", dep_name, mod_name));
                }
                if !visited.contains(dep_id) {
                    visited.insert(*dep_id);
                    queue.push_back(*dep_id);
                }
            }
        }
        dot_src_lines.push("}".to_string());

        let out_file_path = PathBuf::from(&self.options.output_directory)
            .join("img")
            .join(format!(
                "{}_{}_dep.svg",
                module_name,
                (if is_forward { "forward" } else { "backward" })
            ));

        self.gen_svg_file(&out_file_path, &dot_src_lines.join("\n"));
    }

    /// Execute the external tool "dot" with doc_src as input to generate a .svg image file.
    fn gen_svg_file(&mut self, out_file_path: &Path, dot_src: &str) {
        if let Err(e) = fs::create_dir_all(out_file_path.parent().unwrap()) {
            self.unknown_loc_error(format!("cannot create a directory for images ({})", e));
            return;
        }

        let mut child = match Command::new("dot")
            .arg("-Tsvg")
            .args(["-o", out_file_path.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                self.unknown_loc_error(format!(
                    "The Graphviz tool \"dot\" is not available. {}",
                    e
                ));
                return;
            }
        };

        if let Err(e) = child
            .stdin
            .as_mut()
            .ok_or("Child process stdin has not been captured!")
            .unwrap()
            .write_all(dot_src.as_bytes())
        {
            self.unknown_loc_error(format!("{}", e));
            return;
        }

        match child.wait_with_output() {
            Ok(output) => {
                if !output.status.success() {
                    self.unknown_loc_error(format!(
                        "dot failed to generate {}\n{}",
                        out_file_path.to_str().unwrap(),
                        dot_src
                    ));
                }
            }
            Err(e) => {
                self.unknown_loc_error(format!("{}", e));
            }
        }
    }

    /// Generate header for TOC, returning label where we can later insert the content after
    /// file generation is done.
    fn gen_toc_header(&mut self) -> CodeWriterLabel {
        // Create label where we later can insert the TOC
        writeln!(self.writer).unwrap();
        let toc_label = self.writer.create_label();
        writeln!(self.writer).unwrap();
        toc_label
    }

    /// Generate table of content and insert it at label.
    fn gen_toc(&mut self, label: CodeWriterLabel) {
        // We put this into a separate code writer and insert its content at the label.
        let mut writer = std::mem::replace(&mut self.writer, CodeWriter::new());
        {
            let mut level = 0;
            for (nest, entry) in self
                .toc
                .iter()
                .filter(|(n, _)| *n > 0 && *n <= self.options.toc_depth)
                .cloned()
                .collect::<Vec<_>>()
            {
                let n = nest - 1;
                while level < n {
                    self.begin_items();
                    self.writer.indent();
                    level += 1;
                }
                while level > n {
                    self.end_items();
                    self.writer.unindent();
                    level -= 1;
                }
                self.item_text(&format!("[{}](#{})", entry.title, entry.label));
            }
            while level > 0 {
                self.end_items();
                self.writer.unindent();
                level -= 1;
            }
            // Insert the result at label.
            self.writer
                .process_result(|s| writer.insert_at_label(label, s));
        }
        self.writer = writer;
    }

    /// Generate an index of all modules and scripts in the context. This includes generated
    /// ones and those which are only dependencies.
    fn gen_index(&mut self, env: &Model) {
        // Sort all modules and script by simple name. (Perhaps we should include addresses?)
        let sorted_infos = self
            .infos
            .keys()
            .sorted_by_key(|id| env.module(id).id().1)
            .copied()
            .collect::<Vec<_>>();
        self.begin_items();
        for id in sorted_infos {
            let module_env = env.module(id);
            if !matches!(
                module_env.info().target_kind,
                TargetKind::Source {
                    is_root_package: true
                }
            ) {
                // Do not include modules which are not target (outside of the package)
                // into the index.
                continue;
            }
            let ref_for_module = self.ref_for_module(module_env);
            self.item_text(&format!("[`{}`]({})", id.1, ref_for_module))
        }
        self.end_items();
    }

    /// Generates documentation for all named constants.
    fn gen_named_constants(&mut self, env: &Model) {
        let label = self.label_for_section("Constants");
        self.section_header("Constants", &label);
        self.increment_section_nest();
        let current_module = self.current_module.unwrap();
        let current_module = env.module(current_module);
        for const_env in current_module.constants() {
            self.label(&self.label_for_module_item(current_module, const_env.name()));
            self.doc_text(env, const_env.doc());
            self.code_block(env, &self.named_constant_display(const_env));
        }

        self.decrement_section_nest();
    }

    /// Generates documentation for a struct.
    fn gen_struct(&mut self, struct_env: model::Struct<'_>) {
        let env = struct_env.model();
        let module_env = struct_env.module();
        let name = struct_env.name();
        self.section_header("Struct", &self.label_for_module_item(module_env, name));
        self.increment_section_nest();
        self.doc_text(env, struct_env.doc());
        self.code_block(env, &self.struct_header_display(struct_env));

        if self.options.include_impl || (self.options.include_specs && self.options.specs_inlined) {
            // Include field documentation if either impls or specs are present and inlined,
            // because they are used by both.
            self.begin_collapsed("Fields");
            self.gen_struct_fields(struct_env);
            self.end_collapsed();
        }

        self.decrement_section_nest();
    }

    /// Generates documentation for an enum.
    fn gen_enum(&self, enum_env: &EnumEnv<'_>) {
        let name = enum_env.get_name();
        self.section_header(
            &format!("Enum `{}`", self.name_string(enum_env.get_name())),
            &self.label_for_module_item(&enum_env.module_env, name),
        );
        self.increment_section_nest();
        self.doc_text(enum_env.get_doc());
        self.code_block(&self.enum_header_display(enum_env));

        if self.options.include_impl || (self.options.include_specs && self.options.specs_inlined) {
            // Include field documentation if either impls or specs are present and inlined,
            // because they are used by both.
            self.begin_collapsed("Variants");
            self.gen_enum_variants(enum_env);
            self.end_collapsed();
        }

        self.decrement_section_nest();
    }

    /// Returns "Struct `N`" or "Resource `N`".
    fn struct_title(&self, struct_env: &StructEnv<'_>) -> String {
        // NOTE(mengxu): although we no longer declare structs with the `resource` keyword, it
        // might be helpful in keeping `Resource N` in struct title as the boogie translator still
        // depends on the `is_resource()` predicate to add additional functions to structs declared
        // with the `key` ability.
        format!(
            "{} `{}`",
            if struct_env.has_memory() {
                "Resource"
            } else {
                "Struct"
            },
            self.name_string(struct_env.get_name())
        )
    }

    /// Generates declaration for named constant
    fn named_constant_display(&self, const_env: &NamedConstantEnv<'_>) -> String {
        let name = self.name_string(const_env.get_name());
        let is_error_const = const_env.get_attributes().iter().any(|attr|
            matches!(attr, Attribute::Apply(_, sym, _) if self.name_string(*sym).to_string() == *"error")
        );
        let rendered_value = match (is_error_const, const_env.get_value()) {
            (true, Value::ByteArray(bytes)) => {
                if let Ok(s) = std::str::from_utf8(&bytes) {
                    format!("b\"{s}\"")
                } else {
                    format!("{bytes:?}")
                }
            }
            (_, value) => value.to_string(),
        };
        let error_const_annot = if is_error_const { "#[error]\n" } else { "" };
        format!(
            "{}const {}: {} = {};",
            error_const_annot,
            name,
            const_env.get_type().display(&TypeDisplayContext::WithEnv {
                env: self.env,
                type_param_names: None,
            }),
            rendered_value,
        )
    }

    /// Generates code signature for a struct.
    fn struct_header_display(&self, struct_env: model::Struct<'_>) -> String {
        let name = struct_env.name();
        let type_params = struct_env
            .info()
            .type_parameters
            .iter()
            .map(|tp| tp.param.user_specified_name.value)
            .join(", ");
        let ability_tokens = self.ability_tokens(struct_env.struct_handle().abilities);
        if ability_tokens.is_empty() {
            format!("struct {}{}", name, type_params)
        } else {
            format!(
                "struct {}{} has {}",
                name,
                type_params,
                ability_tokens.join(", ")
            )
        }
    }

    fn gen_struct_fields(&mut self, struct_env: model::Struct<'_>) {
        self.begin_definitions();
        let fields = match &struct_env.info().fields {
            move_compiler::naming::ast::StructFields::Defined(_, fields) => fields
                .iter()
                .map(|(_, field, (idx, ty))| (*idx, *field, ty))
                .sorted_by_key(|(idx, _, _)| *idx)
                .collect(),
            move_compiler::naming::ast::StructFields::Native(_) => vec![],
        };
        for (_, field, ty) in fields {
            self.definition_text(
                struct_env.model(),
                &format!("`{}: {}`", field, model_display::type_(ty)),
                struct_env.field_doc(field),
            );
        }
        self.end_definitions();
    }

    /// Generates code signature for an enum.
    fn enum_header_display(&self, enum_env: &EnumEnv<'_>) -> String {
        let name = self.name_string(enum_env.get_name());
        let type_params = self.type_parameter_list_display(&enum_env.get_named_type_parameters());
        let ability_tokens = self.ability_tokens(enum_env.get_abilities());
        if ability_tokens.is_empty() {
            format!("public enum {}{}", name, type_params)
        } else {
            format!(
                "public enum {}{} has {}",
                name,
                type_params,
                ability_tokens.join(", ")
            )
        }
    }

    fn gen_enum_variants(&self, enum_env: &EnumEnv<'_>) {
        let tctx = {
            let type_param_names = Some(
                enum_env
                    .get_named_type_parameters()
                    .iter()
                    .map(|TypeParameter(name, _)| *name)
                    .collect_vec(),
            );
            TypeDisplayContext::WithEnv {
                env: self.env,
                type_param_names,
            }
        };
        self.begin_definitions();
        for variant_env in enum_env.get_variants() {
            self.definition_text(
                &format!("Variant `{}`", self.name_string(variant_env.get_name()),),
                variant_env.get_doc(),
            );
            for field in variant_env.get_fields() {
                self.begin_definitions();
                self.definition_text(
                    &format!(
                        "`{}: {}`",
                        self.name_string(field.get_name()),
                        field.get_type().display(&tctx)
                    ),
                    field.get_doc(),
                );
                self.end_definitions();
            }
        }
        self.end_definitions();
    }

    /// Generates documentation for a function.
    fn gen_function(&mut self, func_env: model::Function<'_>) {
        let env = func_env.model();
        let module_env = func_env.module();
        let name = func_env.name();
        let full_name = format!("{}::{}", module_env.ident(), name);
        self.section_header(
            &format!("Function `{full_name}`"),
            &self.label_for_module_item(module_env, name),
        );
        self.increment_section_nest();
        self.doc_text(env, func_env.doc());
        let sig = self.function_header_display(name, func_env);
        self.code_block(env, &sig);
        if self.options.include_impl {
            self.begin_collapsed("Implementation");
            self.code_block(
                env,
                &self.get_source_with_indent(env, func_env.info().full_loc),
            );
            self.end_collapsed();
        }
        if self.options.include_call_diagrams {
            let file_prefix = full_name.replace("::", "_");
            self.gen_call_diagram(env, module_env.id(), name, true);
            self.begin_collapsed(&format!("Show all the functions that \"{}\" calls", name,));
            self.image(&format!("img/{}_forward_call_graph.svg", file_prefix));
            self.end_collapsed();

            self.gen_call_diagram(env, module_env.id(), name, false);
            self.begin_collapsed(&format!("Show all the functions that call \"{}\"", &name));
            self.image(&format!("img/{}_backward_call_graph.svg", file_prefix));
            self.end_collapsed();
        }
        self.decrement_section_nest();
    }

    /// Generates documentation for a function signature.
    fn function_header_display(&self, name: Symbol, func_env: model::Function<'_>) -> String {
        let signature = &func_env.info().signature;
        let type_params = signature
            .type_parameters
            .iter()
            .map(|tp| tp.user_specified_name.value)
            .join(", ");
        let params = func_env
            .info()
            .signature
            .parameters
            .iter()
            .map(|(_, v, ty)| format!("{}: {}", v.value.name, model_display::type_(ty)))
            .join(", ");
        let return_types = &func_env.info().signature.return_type;
        let return_str = match &return_types.value {
            move_compiler::naming::ast::Type_::Unit => "".to_owned(),
            _ => format!(": {}", model_display::type_(return_types)),
        };
        let entry_str = if func_env.info().entry.is_some() {
            "entry "
        } else {
            ""
        };
        format!(
            "{}{}fun {}{}({}){}",
            func_env.info().visibility,
            entry_str,
            name,
            type_params,
            params,
            return_str
        )
    }

    // ============================================================================================
    // Helpers

    /// Collect tokens in an ability set
    fn ability_tokens(&self, abilities: file_format::AbilitySet) -> Vec<&'static str> {
        let mut ability_tokens = vec![];
        if abilities.has_copy() {
            ability_tokens.push("copy");
        }
        if abilities.has_drop() {
            ability_tokens.push("drop");
        }
        if abilities.has_store() {
            ability_tokens.push("store");
        }
        if abilities.has_key() {
            ability_tokens.push("key");
        }
        ability_tokens
    }

    /// Increments section nest.
    fn increment_section_nest(&mut self) {
        self.section_nest += 1;
    }

    /// Decrements section nest, committing sub-sections to the table-of-contents map.
    fn decrement_section_nest(&mut self) {
        self.section_nest -= 1;
    }

    /// Creates a new section header and inserts a table-of-contents entry into the generator.
    fn section_header(&mut self, s: &str, label: &str) {
        let level = self.section_nest;
        if usize::saturating_add(self.options.section_level_start, level) > MAX_SUBSECTIONS {
            panic!("Maximum number of subheadings exceeded with heading: {}", s)
        }
        if !label.is_empty() {
            self.label(label);
            let entry = TocEntry {
                title: s.to_owned(),
                label: label.to_string(),
            };
            self.toc.push((level, entry));
        }
        writeln!(
            self.writer,
            "{} {}",
            self.repeat_str("#", self.options.section_level_start + level),
            s,
        )
        .unwrap();
        writeln!(self.writer).unwrap();
    }

    /// Includes the image in the given path.
    fn image(&mut self, path: &str) {
        writeln!(self.writer).unwrap();
        writeln!(self.writer, "![]({})", path).unwrap();
        writeln!(self.writer).unwrap();
    }

    /// Generate label.
    fn label(&mut self, label: &str) {
        writeln!(self.writer).unwrap();
        writeln!(self.writer, "<a name=\"{}\"></a>", label).unwrap();
        writeln!(self.writer).unwrap();
    }

    /// Begins a collapsed section.
    fn begin_collapsed(&mut self, summary: &str) {
        writeln!(self.writer).unwrap();
        if self.options.collapsed_sections {
            writeln!(self.writer, "<details>").unwrap();
            writeln!(self.writer, "<summary>{}</summary>", summary).unwrap();
        } else {
            writeln!(self.writer, "##### {}", summary).unwrap();
        }
        writeln!(self.writer).unwrap();
    }

    /// Ends a collapsed section.
    fn end_collapsed(&mut self) {
        if self.options.collapsed_sections {
            writeln!(self.writer).unwrap();
            writeln!(self.writer, "</details>").unwrap();
        }
    }

    /// Outputs documentation text.
    fn doc_text_general(&mut self, env: &Model, for_root: bool, text: &str) {
        for line in self.decorate_text(env, text).lines() {
            let line = line.trim();
            if line.starts_with('#') {
                let mut i = 1;
                while line[i..].starts_with('#') {
                    i += 1;
                    self.increment_section_nest();
                }
                let header = line[i..].trim_start();
                if for_root {
                    self.last_root_section_nest = self.section_nest;
                }
                let label = self.label_for_section(header);
                self.section_header(header, &label);
                while i > 1 {
                    self.decrement_section_nest();
                    i -= 1;
                }
            } else {
                writeln!(self.writer, "{line}").unwrap();
            }
        }
        // Always be sure to have an empty line at the end of block.
        writeln!(self.writer).unwrap();
    }

    fn doc_text_for_root(&mut self, env: &Model, text: &str) {
        self.doc_text_general(env, true, text)
    }

    fn doc_text(&mut self, env: &Model, text: &str) {
        self.doc_text_general(env, false, text)
    }

    /// Makes a label from a string.
    fn make_label_from_str(&self, s: &str) -> String {
        format!("@{}", s.replace(' ', "_"))
    }

    /// Decorates documentation text, identifying code fragments and decorating them
    /// as code. Code blocks in comments are untouched.
    fn decorate_text(&self, env: &Model, text: &str) -> String {
        let mut decorated_text = String::new();
        let mut chars = text.chars();
        let non_code_filter = |chr: &char| *chr != '`';

        while let Some(chr) = chars.next() {
            if chr == '`' {
                // See if this is the start of a code block.
                let is_start_of_code_block = chars.take_while_ref(|chr| *chr == '`').count() > 0;
                if is_start_of_code_block {
                    // Code block -- don't create a <code>text</code> for this.
                    decorated_text += "```";
                } else {
                    // inside inline code section. Eagerly consume/match this '`'
                    let code = chars.take_while_ref(non_code_filter).collect::<String>();
                    // consume the remaining '`'. Report an error if we find an unmatched '`'.
                    assert!(
                        chars.next() == Some('`'),
                        "Missing backtick found in {} while generating \
                        documentation for the following text: \"{}\"",
                        env.module(self.current_module.unwrap()).ident(),
                        text,
                    );

                    write!(
                        &mut decorated_text,
                        "<code>{}</code>",
                        self.decorate_code(env, &code)
                    )
                    .unwrap()
                }
            } else {
                decorated_text.push(chr);
                decorated_text.extend(chars.take_while_ref(non_code_filter))
            }
        }
        decorated_text
    }

    /// Begins a code block. This uses html, not markdown code blocks, so we are able to
    /// insert style and links into the code.
    fn begin_code(&mut self) {
        writeln!(self.writer).unwrap();
        // If we newline after <pre><code>, an empty line will be created. So we don't.
        // This, however, creates some ugliness with indented code.
        write!(self.writer, "<pre><code>").unwrap();
    }

    /// Ends a code block.
    fn end_code(&mut self) {
        writeln!(self.writer, "</code></pre>\n").unwrap();
        // Always be sure to have an empty line at the end of block.
        writeln!(self.writer).unwrap();
    }

    /// Outputs decorated code text in context of a module.
    fn code_text(&mut self, env: &Model, code: &str) {
        writeln!(self.writer, "{}", self.decorate_code(env, code)).unwrap();
    }

    /// Decorates a code fragment, for use in an html block. Replaces < and >, bolds keywords and
    /// tries to resolve and cross-link references.
    fn decorate_code(&self, env: &Model, code: &str) -> String {
        static REX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
                r"(?P<ident>(\b\w+\b\s*::\s*)*\b\w+\b)(?P<call>\s*[(<])?|(?P<lt><)|(?P<gt>>)",
            )
            .unwrap()
        });
        let mut r = String::new();
        let mut at = 0;
        while let Some(cap) = REX.captures(&code[at..]) {
            let replacement = {
                if cap.name("lt").is_some() {
                    "&lt;".to_owned()
                } else if cap.name("gt").is_some() {
                    "&gt;".to_owned()
                } else if let Some(m) = cap.name("ident") {
                    let s = m.as_str();
                    if KEYWORDS.contains(&s)
                        || CONTEXTUAL_KEYWORDS.contains(&s)
                        || BUILTINS.contains(&s)
                    {
                        format!("<b>{}</b>", &code[at + m.start()..at + m.end()])
                    } else if let Some(label) = self.resolve_to_label(env, s) {
                        format!("<a href=\"{}\">{}</a>", label, s)
                    } else {
                        "".to_owned()
                    }
                } else {
                    "".to_owned()
                }
            };
            if replacement.is_empty() {
                r += &code[at..at + cap.get(0).unwrap().end()].replace('<', "&lt;");
            } else {
                r += &code[at..at + cap.get(0).unwrap().start()];
                r += &replacement;
                if let Some(m) = cap.name("call") {
                    // Append the call or generic open we may have also matched to distinguish
                    // a simple name from a function call or generic instantiation. Need to
                    // replace the `<` as well.
                    r += &m.as_str().replace('<', "&lt;");
                }
            }
            at += cap.get(0).unwrap().end();
        }
        r += &code[at..];
        r
    }

    /// Resolve a string of the form `ident`, `ident::ident`, or `0xN::ident::ident` into
    /// the label for the declaration inside of this documentation. This uses a
    /// heuristic and may not work in all cases or produce wrong results (for instance, it
    /// ignores aliases). To improve on this, we would need best direct support by the compiler.
    fn resolve_to_label(&self, env: &Model, mut s: &str) -> Option<String> {
        // For clarity in documentation, we allow `script::` or `module::` as a prefix.
        // However, right now it will be ignored for resolution.
        let lower_s = s.to_lowercase();
        if lower_s.starts_with("script::") {
            s = &s["script::".len()..]
        } else if lower_s.starts_with("module::") {
            s = &s["module::".len()..]
        }
        let parts_data: Vec<&str> = s.splitn(3, "::").collect();
        let mut parts = parts_data.as_slice();
        let module_opt = if parts[0].starts_with("0x") {
            if parts.len() == 1 {
                // Cannot resolve.
                return None;
            }
            let addr = AccountAddress::from_hex_literal(parts[0]).ok()?;
            let mname = (addr, Symbol::from(parts[1]));
            parts = &parts[2..];
            Some(env.maybe_module(mname)?)
        } else {
            None
        };
        let try_func_struct_or_const = |module: model::Module<'_>, name: Symbol| {
            // Below we only resolve a simple name to a hyperref if it is followed by a ( or <,
            // or if it is a named constant in the module.
            // Otherwise we get too many false positives where names are resolved to functions
            // but are actually fields.
            module
                .member(name)
                .map(|_member| self.ref_for_module_item(module, name))
        };
        let parts_sym = parts.iter().map(|p| Symbol::from(*p)).collect_vec();

        match (module_opt, parts_sym.len()) {
            (Some(module), 0) => Some(self.ref_for_module(module)),
            (Some(module), 1) => try_func_struct_or_const(module, parts_sym[0]),
            (None, 0) => None,
            (None, 1) => {
                // A simple name. Resolve either to module or to item in current module.
                let preferred_module = self
                    .preferred_modules
                    .get(&parts_sym[0])
                    .and_then(|addr| env.maybe_module((*addr, parts_sym[0])));
                if let Some(module) = preferred_module {
                    Some(self.ref_for_module(module))
                } else if let Some(module) = &self.current_module {
                    let module = env.module(module);
                    try_func_struct_or_const(module, parts_sym[0])
                } else {
                    None
                }
            }
            (None, 2) => {
                // A qualified name, but without the address. This must be an item in a module
                // denoted by the first name.
                let module_opt = if parts[0] == "Self" {
                    self.current_module
                        .as_ref()
                        .and_then(|id| env.maybe_module(id))
                } else {
                    self.preferred_modules
                        .get(&parts_sym[0])
                        .and_then(|addr| env.maybe_module((*addr, parts_sym[0])))
                };
                if let Some(module) = module_opt {
                    try_func_struct_or_const(module, parts_sym[1])
                } else {
                    None
                }
            }
            (_, _) => None,
        }
    }

    /// Create label for a module.
    fn make_label_for_module(&self, module_env: model::Module<'_>) -> String {
        format!("{}", module_env.ident()).replace("::", "_")
    }

    /// Return the label for a module.
    fn label_for_module(&self, module_env: model::Module<'_>) -> &str {
        let Some(info) = self.infos.get(&module_env.id()) else {
            return "";
        };
        &info.label
    }

    /// Return the reference for a module.
    fn ref_for_module(&self, module_env: model::Module<'_>) -> String {
        let Some(info) = self.infos.get(&module_env.id()) else {
            return String::new();
        };
        let extension = if !self
            .current_module
            .as_ref()
            .map(|id| module_env.model().module(id))
            .map(|x| {
                matches!(
                    x.info().target_kind,
                    TargetKind::Source {
                        is_root_package: true
                    }
                )
            })
            .unwrap_or(true)
        {
            "../../"
        } else {
            ""
        };
        format!("{}{}#{}", extension, info.target_file, info.label)
    }

    /// Return the label for an item in a module.
    fn label_for_module_item(&self, module_env: model::Module<'_>, item: Symbol) -> String {
        self.label_for_module_item_str(module_env, item.as_str())
    }

    /// Return the label for an item in a module.
    fn label_for_module_item_str(&self, module_env: model::Module<'_>, s: &str) -> String {
        format!("{}_{}", self.label_for_module(module_env), s)
    }

    /// Return the reference for an item in a module.
    fn ref_for_module_item(&self, module_env: model::Module<'_>, item: Symbol) -> String {
        format!("{}_{}", self.ref_for_module(module_env), item)
    }

    /// Create a unique label for a section header.
    fn label_for_section(&mut self, title: &str) -> String {
        let counter = self.label_counter;
        self.label_counter += 1;
        self.make_label_from_str(&format!("{} {}", title, counter))
    }

    /// Shortcut for code_block in a module context.
    fn code_block(&mut self, env: &Model, code: &str) {
        self.begin_code();
        self.code_text(env, code);
        self.end_code();
    }

    /// Begin an itemized list.
    fn begin_items(&self) {}

    /// End an itemized list.
    fn end_items(&self) {}

    /// Emit an item.
    fn item_text(&mut self, text: &str) {
        writeln!(self.writer, "-  {}", text).unwrap();
    }

    /// Begin a definition list.
    fn begin_definitions(&mut self) {
        writeln!(self.writer).unwrap();
        writeln!(self.writer, "<dl>").unwrap();
    }

    /// End a definition list.
    fn end_definitions(&mut self) {
        writeln!(self.writer, "</dl>").unwrap();
        writeln!(self.writer).unwrap();
    }

    /// Emit a definition.
    fn definition_text(&mut self, env: &Model, term: &str, def: &str) {
        writeln!(
            self.writer,
            "<dt>\n{}\n</dt>",
            self.decorate_text(env, term)
        )
        .unwrap();
        writeln!(self.writer, "<dd>\n{}\n</dd>", self.decorate_text(env, def)).unwrap();
    }

    /// Retrieves source of code fragment with adjusted indentation.
    /// Typically code has the first line unindented because location tracking starts
    /// at the first keyword of the item (e.g. `public fun`), but subsequent lines are then
    /// indented. This uses a heuristic by guessing the indentation from the context.
    fn get_source_with_indent(&self, env: &Model, loc: Loc) -> String {
        let files = env.files();
        let source = files.source(&loc.file_hash()).unwrap();
        let source: &str = source.as_ref();
        // Compute the indentation of this source fragment by looking at some
        // characters preceding it.
        let ByteSpan { start, end } = files.byte_location(loc).byte_span;
        let source = &source[start..end];
        let peek_start = start.saturating_sub(60);
        let source_before = &source[peek_start..start];
        let newl_at = source_before.rfind('\n').unwrap_or(0);
        let mut indent = source_before.len() - newl_at - 1;
        if indent >= 4 && source_before.ends_with("spec ") {
            // Special case for `spec define` and similar constructs.
            indent -= 4;
        }
        // Remove the indent from all lines.
        source
            .lines()
            .map(|l| {
                let mut i = 0;
                while i < indent && i < l.len() && l[i..].starts_with(' ') {
                    i += 1;
                }
                &l[i..]
            })
            .join("\n")
    }

    /// Repeats a string n times.
    fn repeat_str(&self, s: &str, n: usize) -> String {
        (0..n).map(|_| s).collect::<String>()
    }
}
