// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{command::CommandOptions, run_cmd};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use inquire::Text;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use toml_edit::Document;
use toml_edit::{value, ArrayOfTables, Item, Table};
use tracing::{debug, error, info, warn};

pub enum ProjectType {
    App,
    Basic,
    CronJob,
}

const KEYRING: &str = "pulumi-kms-automation-f22939d";

impl ProjectType {
    pub fn create_project(&self, use_kms: &bool) -> Result<()> {
        // make sure we're in suiops
        ensure_in_suiops_repo()?;
        // inquire params from user
        let project_name = Text::new("project name:").prompt()?;
        // create dir
        let project_subdir = match self {
            Self::App | Self::CronJob => "apps",
            Self::Basic => "services",
        };
        let project_dir = get_pulumi_dir()?.join(project_subdir).join(&project_name);
        let mut project_opts = vec![];
        if *use_kms {
            let encryption_key_id = get_encryption_key_id(&project_name)?;
            project_opts.push(format!("--secrets-provider=gcpkms://{}", encryption_key_id));
        }
        if project_dir.exists() {
            Err(anyhow!(
                "{} already exists. Please use a project name that doesn't already exist",
                project_dir
                    .to_str()
                    .expect("project dir to str")
                    .bright_purple()
            ))
        } else {
            match self {
                Self::App => {
                    info!("creating k8s containerized application");
                    create_mysten_k8s_project(
                        &project_name,
                        &project_dir,
                        Self::App,
                        &project_opts,
                    )?;
                }
                Self::Basic => {
                    info!("creating basic pulumi project");
                    create_basic_project(&project_name, &project_dir, &project_opts)?;
                }
                Self::CronJob => {
                    info!("creating k8s cronjob project");
                    create_mysten_k8s_project(
                        &project_name,
                        &project_dir,
                        Self::CronJob,
                        &project_opts,
                    )?;
                }
            }
            info!("your new project is ready to go!");
            Ok(())
        }
    }
}

fn ensure_in_suiops_repo() -> Result<()> {
    let remote_stdout = run_cmd(
        vec!["git", "config", "--get", "remote.origin.url"],
        Some(CommandOptions::new(false, false)),
    )
    .context("run this command within the sui-operations repository")?
    .stdout;
    let in_suiops = String::from_utf8_lossy(&remote_stdout)
        .trim()
        .contains("sui-operations");
    if !in_suiops {
        Err(anyhow!(
            "please run this command from within the sui-operations repository"
        ))
    } else {
        Ok(())
    }
}

fn get_pulumi_dir() -> Result<PathBuf> {
    let suiops_dir_stdout = run_cmd(
        vec!["git", "rev-parse", "--show-toplevel"],
        Some(CommandOptions::new(false, false)),
    )
    .context("run this command from within the sui-operations repository")?
    .stdout;
    let suiops_dir = PathBuf::from(String::from_utf8_lossy(&suiops_dir_stdout).trim());
    Ok(suiops_dir.join("pulumi"))
}

fn run_pulumi_new(
    project_name: &str,
    project_dir_str: &str,
    project_opts: &[String],
) -> Result<()> {
    info!(
        "creating new pulumi project in {}",
        project_dir_str.bright_purple()
    );
    let opts = project_opts.join(" ");
    info!("extra pulumi options added: {}", &opts.bright_purple());
    run_cmd(
        vec![
            "bash",
            "-c",
            &format!(
                r#"pulumi new python --dir {0} -d "pulumi project for {1}" --name "{1}"  --stack dev --yes {2}"#,
                project_dir_str, project_name, opts
            ),
        ],
        None,
    )?;
    Ok(())
}

fn run_pulumi_new_from_template(
    project_name: &str,
    project_dir_str: &str,
    project_type: ProjectType,
    project_opts: &[String],
) -> Result<()> {
    info!(
        "creating new pulumi project in {}",
        project_dir_str.bright_purple()
    );
    let template_dir = match project_type {
        ProjectType::App => "container_app",
        ProjectType::CronJob => "cronjob",
        _ => "container_app",
    };
    let opts = project_opts.join(" ");
    info!("extra pulumi options added: {}", &opts.bright_purple());
    run_cmd(
        vec![
            "bash",
            "-c",
            &format!(
                r#"pulumi new {3}/templates/{2} --dir {0} -d "pulumi project for {1}" --name "{1}"  --stack dev {4}"#,
                project_dir_str,
                project_name,
                template_dir,
                get_pulumi_dir()?
                    .to_str()
                    .expect("getting pulumi dir for template"),
                opts,
            ),
        ],
        Some(CommandOptions::new(true, false)),
    )?;
    Ok(())
}

fn run_poetry_init(project_name: &str, project_dir_str: &str) -> Result<()> {
    info!("initializing poetry in {}", project_dir_str.bright_purple());
    run_cmd(
        vec![
            "bash",
            "-c",
            &format!(
                r#"poetry init -C {0} --python "^3.11" --name {1} --description "pulumi project for {1}" --author "mysten labs <info@mystenlabs.com>" -n"#,
                project_dir_str, project_name,
            ),
        ],
        None,
    ).context("failed running poetry init command")?;
    Ok(())
}

fn move_venv_to_poetry_dir(project_dir: &Path) -> Result<()> {
    let venv_path = project_dir.join("venv");
    let updated = venv_path.with_file_name(".venv");
    fs::rename(&venv_path, &updated).context(format!(
        "couldn't rename {:?} to {:?}",
        &venv_path, &updated
    ))?;
    Ok(())
}

fn make_poetry_in_project() -> Result<()> {
    let home = env::var("HOME").context("HOME env var didn't exist")?;
    let poetry_config_dir =
        PathBuf::from(home).join(PathBuf::from("Library/Application Support/pypoetry"));
    let poetry_config_filepath = poetry_config_dir.join("config.toml");
    if !poetry_config_dir.exists() {
        fs::create_dir(poetry_config_dir).context("couldn't create poetry config dir")?;
    }
    if !poetry_config_filepath.exists() {
        fs::write(&poetry_config_filepath, "").context("couldn't create poetry config file")?;
    }
    let config_contents =
        fs::read(&poetry_config_filepath).context("couldn't read config contents")?;
    let mut config_toml = std::str::from_utf8(&config_contents)
        .context("failed to parse config contents")?
        .parse::<Document>()
        .expect("invalid toml");
    debug!("before changes {:?}", config_toml.to_string());
    config_toml["virtualenvs"]["in-project"] = value(true);
    fs::write(&poetry_config_filepath, config_toml.to_string())
        .context("failed to write poetry config back after update")?;
    Ok(())
}

fn clean_up_pre_poetry_artifacts(project_dir: &Path) -> Result<()> {
    let requirements_txt = project_dir.join("requirements.txt");
    fs::remove_file(requirements_txt)?;
    let pulumi_yaml_path = project_dir.join("Pulumi.yaml");
    let pulumi_yaml_contents = fs::read_to_string(&pulumi_yaml_path)?;
    let mut pulumi_yaml: serde_yaml::Value = serde_yaml::from_str(&pulumi_yaml_contents)?;
    pulumi_yaml["runtime"]["options"]["virtualenv"] = serde_yaml::to_value(".venv")?;
    fs::write(pulumi_yaml_path, serde_yaml::to_string(&pulumi_yaml)?)?;
    Ok(())
}

fn add_standard_dependencies(project_dir_str: &str) -> Result<()> {
    info!("running poetry add");
    run_cmd(
        vec![
            "bash",
            "-c",
            &format!(
                "poetry add ../../common pulumi pulumi-kubernetes -C {}",
                project_dir_str,
            ),
        ],
        None,
    )?;
    Ok(())
}

fn run_poetry_install(project_dir_str: &str) -> Result<()> {
    info!("running poetry install");
    run_cmd(
        vec![
            "bash",
            "-c",
            &format!("poetry install -C {}", project_dir_str,),
        ],
        None,
    )?;
    Ok(())
}

pub fn adjust_pyproject(project_dir: &Path) -> Result<()> {
    info!("setting up pyproject.toml");
    let pyproject_toml_filepath = project_dir.join("pyproject.toml");
    let pyproject_contents =
        fs::read(&pyproject_toml_filepath).context("couldn't read config contents")?;
    let mut pyproject_toml = std::str::from_utf8(&pyproject_contents)
        .context("failed to parse pyproject contents")?
        .parse::<Document>()
        .expect("invalid toml");
    debug!("before changes {:?}", pyproject_toml.to_string());
    // remove the readme definition
    pyproject_toml["tool"]["poetry"]
        .as_table_mut()
        .expect("tool.poetry was not a table")
        .remove("readme");
    // include all python files
    let mut package_table = Table::new();
    package_table.insert("include", value("**/*.py"));
    let mut package_array = ArrayOfTables::new();
    package_array.push(package_table);
    pyproject_toml["tool"]["poetry"]["packages"] =
        Item::Value(toml_edit::Value::Array(package_array.into_array()));
    fs::write(&pyproject_toml_filepath, pyproject_toml.to_string())
        .context("failed to write pyproject.toml back after update")?;
    Ok(())
}

fn run_pulumi_preview(project_dir_str: &str) -> Result<()> {
    info!("running pulumi preview to make sure everything is functional");
    run_cmd(
        vec![
            "bash",
            "-c",
            &format!("pulumi preview -C {}", project_dir_str),
        ],
        None,
    )?;
    Ok(())
}

fn remove_project_dir(project_dir: &PathBuf) -> Result<()> {
    fs::remove_dir_all(project_dir).context("removing project dir")
}

#[derive(Serialize, Deserialize)]
struct PulumiBackendURL {
    url: String,
}

fn get_current_backend() -> Result<String> {
    debug!("running pulumi whoami -v -j to get the Backend URL");
    let output = run_cmd(vec!["bash", "-c", "pulumi whoami -v -j"], None)?;
    let stdout_str = std::str::from_utf8(&output.stdout)?;
    let val: PulumiBackendURL = serde_json::from_str(stdout_str)?;
    let mut parts = val.url.split("com/");
    if let Some(answer) = parts.nth(1) {
        if !answer.is_empty() {
            let ret = answer.trim().to_string();
            debug!("Backend URL is pointing to: {ret}");
            Ok(ret)
        } else {
            Err(anyhow!("Backend URL is incomplete"))
        }
    } else {
        Err(anyhow!("Backend URL is incomplete"))
    }
}

fn remove_stack(backend: &str, project_name: &str, stack_name: &str) -> Result<()> {
    let stack_str = format!("{}/{}/{}", backend, project_name, stack_name);
    debug!("cleaning up {}...", &stack_str);
    run_cmd(
        vec![
            "bash",
            "-c",
            &format!("pulumi stack rm {} --yes", stack_str),
        ],
        None,
    )?;
    warn!(
        "{} cleaned up successfully, you'll have a clean start next time.",
        &stack_str.bright_purple()
    );
    Ok(())
}

fn create_basic_project(
    project_name: &str,
    project_dir: &PathBuf,
    project_opts: &[String],
) -> Result<()> {
    let project_dir_str = project_dir.to_str().expect("project dir to str");
    info!(
        "creating project directory at {}",
        project_dir_str.bright_purple()
    );
    fs::create_dir_all(project_dir).context("failed to create project directory")?;
    // initialize pulumi project
    run_pulumi_new(project_name, project_dir_str, project_opts).map_err(|e| {
        remove_project_dir(project_dir).unwrap();
        let backend = get_current_backend().unwrap();
        remove_stack(&backend, project_name, "dev").unwrap();
        e
    })?;
    // initialize poetry in project dir
    run_poetry_init(project_name, project_dir_str).map_err(|e| {
        remove_project_dir(project_dir).unwrap();
        let backend = get_current_backend().unwrap();
        remove_stack(&backend, project_name, "dev").unwrap();
        e
    })?;
    // update venv to .venv (standard Poetry location)
    move_venv_to_poetry_dir(project_dir)?;
    // add `in-project = true` to config
    // (allows using `pulumi {command}` instead of `poetry run pulumi {command}`)
    make_poetry_in_project()?;
    // clean up pre-poetry project (rm requirements.txt and update Pulumi.yaml)
    clean_up_pre_poetry_artifacts(project_dir)?;
    // add mysten common lib and standard deps
    add_standard_dependencies(project_dir_str)?;
    // fix pyproject.toml file to work for our project layout
    adjust_pyproject(project_dir)?;
    // install poetry dependencies that might have been missed
    run_poetry_install(project_dir_str)?;
    // try a pulumi preview to make sure it's good
    run_pulumi_preview(project_dir_str)
}

fn create_mysten_k8s_project(
    project_name: &str,
    project_dir: &PathBuf,
    project_type: ProjectType,
    project_opts: &[String],
) -> Result<()> {
    let project_dir_str = project_dir.to_str().expect("project dir to str");
    info!(
        "creating project directory at {}",
        project_dir_str.bright_purple()
    );
    fs::create_dir_all(project_dir).context("failed to create project directory")?;
    // initialize pulumi project
    run_pulumi_new_from_template(project_name, project_dir_str, project_type, project_opts)
        .map_err(|e| {
            remove_project_dir(project_dir).unwrap();
            let backend = get_current_backend().unwrap();
            remove_stack(&backend, project_name, "dev").unwrap();
            e
        })?;
    // install poetry dependencies that might have been missed
    run_poetry_install(project_dir_str)
    // we don't run preview for templated apps because the user
    // has to give the repo dir (improvements to this coming soon)
}

#[derive(Serialize, Deserialize)]
struct KMSKeyPrimary {
    state: String,
    algorithm: String,
}
#[derive(Serialize, Deserialize)]
struct KMSKey {
    name: String,
    primary: KMSKeyPrimary,
}
fn get_encryption_key_id(project_name: &str) -> Result<String> {
    let output = run_cmd(
        vec![
            "bash",
            "-c",
            &format!(
                "gcloud kms keys list --location=global --keyring={} --format json",
                KEYRING
            ),
        ],
        None,
    )
    .map_err(|e| {
        error!(
            "Cannot list KMS keys, please add your Google account to {}",
            "pulumi/meta/gcp-iam-automation/config.toml".bright_yellow()
        );
        e
    })?;
    let stdout_str = String::from_utf8(output.stdout)?;
    let keys: Vec<KMSKey> = serde_json::from_str(&stdout_str)?;
    for key in keys {
        let key_id = key.name;
        if let Some((_, key_name)) = key_id.rsplit_once('/') {
            if key_name.starts_with(project_name)
                && key.primary.state == "ENABLED"
                && key.primary.algorithm == "GOOGLE_SYMMETRIC_ENCRYPTION"
            {
                return Ok(key_id);
            }
        }
    }
    error!(
        "Cannot find encryption key matching project name: {0}.\n
        Please add a new entry {1} to {2}, create a PR then land it.\n
        A Github workflow will be triggered automatically to create a key for this pulumi project.",
        project_name.bright_purple(),
        format!("[kms.{}]", project_name).bright_purple(),
        "pulumi/meta/gcp-iam-automation/config.toml".bright_yellow()
    );
    Err(anyhow!("Missing encryption key"))
}
