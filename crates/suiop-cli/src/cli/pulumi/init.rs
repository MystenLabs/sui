// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::cli::lib::utils::validate_project_name;
use crate::cli::lib::FilePathCompleter;
use crate::{command::CommandOptions, run_cmd};
use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use inquire::Text;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info, warn};

use super::PulumiProjectRuntime;

#[derive(clap::Subcommand, Clone, Debug)]
pub enum ProjectType {
    App,
    Service,
    Basic,
    CronJob,
}

const KEYRING: &str = "pulumi-kms-automation-f22939d";

impl ProjectType {
    pub fn create_project(
        &self,
        use_kms: &bool,
        project_name: Option<String>,
        runtime: &PulumiProjectRuntime,
    ) -> Result<()> {
        // make sure we're in suiops
        let suiops_path = ensure_in_suiops_repo()?;
        info!("suipop path: {}", suiops_path);
        // inquire params from user
        let mut project_name = project_name
            .unwrap_or_else(|| {
                Text::new("project name:")
                    .prompt()
                    .expect("couldn't get project name")
            })
            .trim()
            .to_string();

        // Loop until project_name user input is valid
        loop {
            match validate_project_name(&project_name) {
                Ok(_) => break,
                Err(msg) => {
                    println!("Validation error: {msg}");
                    project_name = Text::new("Please enter a valid project name:")
                        .prompt()
                        .expect("couldn't get project name")
                        .trim()
                        .to_string();
                }
            }
        }

        // create dir
        let project_subdir = match self {
            Self::App | Self::CronJob => "apps".to_owned(),
            Self::Service => "services".to_owned(),
            Self::Basic => Text::new("project subdir:")
                .with_initial_value(&format!("{}/pulumi/", suiops_path))
                .with_autocomplete(FilePathCompleter::default())
                .prompt()
                .expect("couldn't get subdir")
                .trim()
                .to_string(),
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
                Self::App | Self::Service => {
                    info!("creating k8s containerized application/service");
                    create_mysten_k8s_project(
                        &project_name,
                        &project_dir,
                        Self::App,
                        &project_opts,
                        runtime,
                    )?;
                }
                Self::Basic => {
                    info!("creating basic pulumi project");
                    create_basic_project(&project_name, &project_dir, &project_opts, runtime)?;
                }
                Self::CronJob => {
                    info!("creating k8s cronjob project");
                    create_mysten_k8s_project(
                        &project_name,
                        &project_dir,
                        Self::CronJob,
                        &project_opts,
                        runtime,
                    )?;
                }
            }
            info!("your new project is ready to go!");
            Ok(())
        }
    }
}

fn ensure_in_suiops_repo() -> Result<String> {
    let remote_stdout = run_cmd(
        vec!["git", "config", "--get", "remote.origin.url"],
        Some(CommandOptions::new(false, false)),
    )
    .context("run this command within the sui-operations repository")?
    .stdout;
    let raw_path = String::from_utf8_lossy(&remote_stdout);
    let in_suiops = raw_path.trim().contains("sui-operations");
    if !in_suiops {
        Err(anyhow!(
            "please run this command from within the sui-operations repository"
        ))
    } else {
        info!("raw path: {}", raw_path.trim());
        let cmd = &run_cmd(vec!["git", "rev-parse", "--show-toplevel"], None)?.stdout;
        let repo_path = String::from_utf8_lossy(cmd);
        Ok(Path::new(repo_path.trim()).to_str().unwrap().to_string())
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
    runtime: &PulumiProjectRuntime,
) -> Result<()> {
    info!(
        "creating new pulumi project in {}",
        project_dir_str.bright_purple()
    );
    let opts = project_opts.join(" ");
    info!("extra pulumi options added: {}", &opts.bright_purple());
    let runtime_arg = match runtime {
        PulumiProjectRuntime::Go => "go",
        PulumiProjectRuntime::Typescript => "ts",
    };
    run_cmd(
        vec![
            "bash",
            "-c",
            &format!(
                r#"pulumi new {runtime_arg} --dir {0} -d "pulumi project for {1}" --name "{1}"  --stack mysten/dev --yes {2}"#,
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
    runtime: &PulumiProjectRuntime,
) -> Result<()> {
    info!(
        "creating new pulumi project in {}",
        project_dir_str.bright_purple()
    );
    let template_dir = match (project_type, runtime) {
        (ProjectType::App | ProjectType::Service, PulumiProjectRuntime::Go) => "app-go",
        (ProjectType::CronJob, PulumiProjectRuntime::Go) => "cronjob-go",
        (ProjectType::App | ProjectType::Service, PulumiProjectRuntime::Typescript) => "app-ts",
        _ => panic!("unsupported runtime for this project type"),
    };
    let opts = project_opts.join(" ");
    info!("extra pulumi options added: {}", &opts.bright_purple());
    let cmd = &format!(
        r#"pulumi new {3}/templates/{2} --dir {0} -d "pulumi project for {1}" --name "{1}"  --stack mysten/dev {4}"#,
        project_dir_str,
        project_name,
        template_dir,
        get_pulumi_dir()?
            .to_str()
            .expect("getting pulumi dir for template"),
        opts,
    );
    info!("running command: {}", cmd.bright_purple());

    run_cmd(
        vec!["bash", "-c", cmd],
        Some(CommandOptions::new(true, false)),
    )?;
    Ok(())
}

fn run_go_mod_tidy(project_dir_str: &str) -> Result<()> {
    let cmd = &format!("cd {} && go mod tidy", project_dir_str);
    info!(
        "running `{}` to make sure all Golang dependencies are installed.",
        cmd
    );
    run_cmd(vec!["bash", "-c", cmd], None)?;
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

fn set_pulumi_env(project_dir_str: &str) -> Result<()> {
    let cmd = &format!(
        "cd {} && pulumi config env add gcp-app-env --yes",
        project_dir_str
    );
    info!(
        "setting up pulumi environment in {}",
        project_dir_str.bright_purple()
    );
    run_cmd(vec!["bash", "-c", cmd], None)?;
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
    runtime: &PulumiProjectRuntime,
) -> Result<()> {
    let project_dir_str = project_dir.to_str().expect("project dir to str");
    info!(
        "creating project directory at {}",
        project_dir_str.bright_purple()
    );
    fs::create_dir_all(project_dir).context("failed to create project directory")?;
    // initialize pulumi project
    run_pulumi_new(project_name, project_dir_str, project_opts, runtime).inspect_err(|_| {
        remove_project_dir(project_dir).unwrap();
        let backend = get_current_backend().unwrap();
        remove_stack(&backend, project_name, "mysten/dev").unwrap();
    })?;
    // run go mod tidy to make sure all dependencies are installed
    if runtime == &PulumiProjectRuntime::Go {
        debug!("running go mod tidy");
        run_go_mod_tidy(project_dir_str)?;
    }
    // set pulumi env
    set_pulumi_env(project_dir_str)?;
    // try a pulumi preview to make sure it's good
    run_pulumi_preview(project_dir_str)
}

fn create_mysten_k8s_project(
    project_name: &str,
    project_dir: &PathBuf,
    project_type: ProjectType,
    project_opts: &[String],
    runtime: &PulumiProjectRuntime,
) -> Result<()> {
    let project_dir_str = project_dir.to_str().expect("project dir to str");
    info!(
        "creating project directory at {}",
        project_dir_str.bright_purple()
    );
    fs::create_dir_all(project_dir).context("failed to create project directory")?;
    // initialize pulumi project
    run_pulumi_new_from_template(
        project_name,
        project_dir_str,
        project_type,
        project_opts,
        runtime,
    )
    .inspect_err(|_| {
        remove_project_dir(project_dir).unwrap();
        let backend = get_current_backend().unwrap();
        remove_stack(&backend, project_name, "mysten/dev").unwrap();
    })?;
    // run go mod tidy to make sure all dependencies are installed
    if runtime == &PulumiProjectRuntime::Go {
        debug!("running go mod tidy");
        run_go_mod_tidy(project_dir_str)?;
    }
    // we don't run preview for templated apps because the user
    // has to give the repo dir (improvements to this coming soon)

    // set pulumi env
    set_pulumi_env(project_dir_str)
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
    .inspect_err(|_| {
        error!(
            "Cannot list KMS keys, please add your Google account to {}",
            "pulumi/meta/gcp-iam-automation/config.toml".bright_yellow()
        );
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
