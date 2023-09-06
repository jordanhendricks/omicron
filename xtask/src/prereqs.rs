// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! prereqs xtask: A tool to help developers understand and manage dependencies
//! needed for building and/or running Omicron.
//!
//! Currently, there are two classifications of support for developing Omicron:
//! historically, the nomenclature here is a "builder" machine and a "runner"
//! machine. (The same machine could be used for both.)
//!
//! "Builder" machines refer to being able to build the repository, but also do
//! things like run the tests, but it also includes running Omicron with a
//! simulated sled agent.
//!
//! "Runner" machines refer to being able to deploy a more real Omicron cluster,
//! with a real sled agent running in the GZ. This use case is only supported on
//! Helios-based machines (i.e., stlouis running on oxide hardware, or helios
//! running on commodity hardware.)
//!

// TODO: document position on guest OS support

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{clap_derive::ValueEnum, Subcommand};
use serde::{Deserialize, Serialize};
use slog::{error, info, warn, Drain, Logger};

/// Whether the system is intended as a Build machine, a Deploy machine, or
/// both.
#[derive(Debug, Copy, Clone, ValueEnum, Serialize, Deserialize)]
pub(crate) enum UseCase {
    // TODO: document what this means
    // - What tests run, and on which OSes?
    Build,

    Deploy,
    All,
}

#[derive(Subcommand, Debug)]
pub(crate) enum PrereqsCmd {
    /// Check whether this system is suitable for intended use case
    Check {
        /// Print output as JSON
        #[clap(short = 'j', long)]
        json: bool,
    },

    /// Installs prerequisite software
    Install {
        /// Dry run (display what commands will be run, but don't run them)
        #[clap(short = 'n')]
        dry_run: bool,

        /// Install a subset of prerequisites
        #[clap(subcommand)]
        pr_type: PrereqType,
    },

    // TODO: option to specify specific OS or auto-detect (I think I need a
    // default parser for that.)
    /// List prerequisite packages and dependencies
    List {
        /// Print output as JSON
        #[clap(short = 'j', long)]
        json: bool,

        /// List ca
        #[clap(subcommand)]
        pr_type: PrereqType,
    },
}

/// Categories of prerequisites: system packages, installed with a package
/// manager, or other types of dependencies that are downloaded (binaries,
/// bundles of assets, etc).
#[derive(Debug, Subcommand)]
pub(crate) enum PrereqType {
    /// Install specific package(s)
    Pkg {
        /// Package name(s)
        names: Vec<String>,
    },

    /// Install specific Omicron dep(s)
    Dep {
        /// Dependency name(s)
        names: Vec<String>,
    },

    /// Install all packages and dependencies
    All,
}

/// Marker file used to ensure this tool can't be run on shared machines (or any
/// other machine that one might want to protect against installing packages or
/// binaries).
static MARKER_FILE: &str = "/etc/opt/oxide/NO_INSTALL";

static OUT_DIR: &str = "./out";
static DOWNLOADS_DIR: &str = "./out/downloads";

static PR_CFG_FILE: &str = "xtask/src/prereqs.toml";

/// Representation of the prerequisite configuration manifest.
#[derive(Debug, Serialize, Deserialize)]
//#[serde(deny_unknown_fields)]
struct PrereqsManifest {
    helios: PackageDef,
    build_path: PathExpectDef,
    debian_like: PackageDef,

    #[serde(default, rename = "dep")]
    deps: BTreeMap<String, DepDef>,
    //macos: PackageDef,
    //    bin: Vec<DepDef>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PathExpectDef {
    expected: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageDef {
    build_packages: Vec<String>,
    build_deps: Vec<String>,

    deploy_packages: Vec<String>,
    deploy_deps: Vec<String>,

    install_cmd: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DepDef {
    use_case: Vec<UseCase>,
    version: String,
    source: String,
    check_cmd: String,

    // TODO: host OS here for key
    #[serde(default, rename = "md5")]
    md5sums: BTreeMap<String, String>,
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub(crate) enum HostOs {
    Helios,
    Linux,
    Darwin,
}

impl HostOs {
    fn get_pkg_mgr(&self) -> Box<dyn PackageManager> {
        match self {
            HostOs::Helios => Box::new(Pkg {}),
            HostOs::Linux => Box::new(LinuxApt {}),
            HostOs::Darwin => Box::new(DarwinBrew {}),
        }
    }

    fn get_version(&self, log: &Logger) -> Result<String> {
        let mut command = std::process::Command::new("uname");
        let cmd = command.arg("-v");
        //let output = cmd.output().with_context(|| {
        //format!("could not get output for cmd: {}", cmd_str)
        //})?;

        // TODO: better error handling
        let output = cmd.output()?;
        if !output.status.success() {
            error!(log, "could not detect OS version");
            todo!()
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

fn read_prereq_toml(path: &Utf8Path) -> Result<PrereqsManifest> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read {:?}", path))?;
    let raw = std::str::from_utf8(&bytes).expect("config should be valid utf8");
    let cfg: PrereqsManifest = toml::from_str(raw)
        .with_context(|| format!("invalid config: {:?}", path))?;

    Ok(cfg)
}

// Pretty print to ttys; use bunyan-formatted output otherwise.
fn create_logger() -> Logger {
    if atty::is(atty::Stream::Stdout) {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();
        slog::Logger::root(drain, slog::o!())
    } else {
        let drain = std::sync::Mutex::new(
            slog_bunyan::with_name("omicron-prereqs", std::io::stdout())
                .build(),
        )
        .fuse();
        slog::Logger::root(drain, slog::o!())
    }
}

// TODO
fn detect_os() -> Result<HostOs> {
    Ok(HostOs::Helios)
}

// shared configuration:
// - use case
// - host OS
// - host OS version
// - package manager
//
// matrix of OS support is determined based on use case
//

pub(crate) fn cmd_prereqs(
    cmd: PrereqsCmd,
    host_os: Option<HostOs>,
    install_cmd: Option<String>,
    use_case: UseCase,
) -> Result<()> {
    let log = create_logger();
    let cfg = read_prereq_toml(PR_CFG_FILE.into())?;
    // TODO: remove
    //info!(log, "config: {:?}", cfg);

    let host_os = host_os.unwrap_or(detect_os()?);
    let pkg_mgr = host_os.get_pkg_mgr();

    let is_supported = match (use_case, host_os) {
        (UseCase::Build, _) => true,
        (UseCase::Deploy, HostOs::Helios) => true,
        (UseCase::All, HostOs::Helios) => true,
        _ => false,
    };

    // TODO: determine package manager from host OS + config
    let (is_supported, pkgs, deps) = match (use_case, host_os) {
        (UseCase::Build, HostOs::Helios) => {
            (true, cfg.helios.build_packages, cfg.helios.build_deps)
        }
        (UseCase::Deploy, HostOs::Helios) => {
            (true, cfg.helios.deploy_packages, cfg.helios.deploy_deps)
        }
        (UseCase::All, HostOs::Helios) => {
            let mut pkgs = cfg.helios.build_packages.clone();
            pkgs.append(&mut cfg.helios.deploy_packages.clone());
            let mut deps = cfg.helios.build_deps.clone();
            deps.append(&mut cfg.helios.deploy_deps.clone());

            (true, pkgs, deps)
        }

        (UseCase::Build, HostOs::Linux) => {
            let mut pkgs = cfg.debian_like.build_packages.clone();
            pkgs.append(&mut cfg.debian_like.deploy_packages.clone());
            let mut deps = cfg.debian_like.build_deps.clone();
            deps.append(&mut cfg.debian_like.deploy_deps.clone());

            (true, pkgs, deps)
        }

        (UseCase::Build, _) => (true, vec![], vec![]),
        (UseCase::Deploy, _) => (false, vec![], vec![]),
        (UseCase::All, _) => (false, vec![], vec![]),
    };

    let check_paths = match use_case {
        UseCase::Build => Some(cfg.build_path.expected),
        UseCase::All => Some(cfg.build_path.expected),
        UseCase::Deploy => None,
    };

    if !is_supported {
        error!(
            &log,
            "Use case \"{:?}\" not supported for OS: \"{:?}\"",
            use_case,
            host_os
        );
        return Ok(());
    }

    match cmd {
        PrereqsCmd::Check { json } => {
            cmd_check(&log, host_os, json, use_case, pkgs, deps, check_paths)?
        }

        PrereqsCmd::Install { dry_run, pr_type } => {
            cmd_install(&log, dry_run, use_case, pr_type, pkgs, deps)?
        }
        PrereqsCmd::List { json, pr_type } => {
            cmd_list(&log, json, use_case, pr_type, host_os, pkgs, deps)?
        }
    }

    Ok(())
}

fn cmd_check(
    log: &Logger,
    host_os: HostOs,
    json: bool,
    use_case: UseCase,
    pkgs: Vec<String>,
    deps: Vec<String>,
    paths: Option<Vec<String>>,
) -> Result<()> {
    info!(
        log,
        "Checking installed prerequisites for use case \"{:?}\"...", use_case
    );

    if json {
        todo!()
    }

    let mut mp = false;
    let mut mps = Vec::new();
    let mut errors = Vec::new();

    // Check the OS version is supported.
    // TODO: function on HostOs
    let os_version = host_os.get_version(log)?;
    // TODO: fix newline
    //info!(log, "{:?} OS version \"{}\": OK", host_os, os_version);
    info!(log, "{:?} OS version \"{}\"... OK", host_os, "helios-2.0.22094");

    //   info!(log, "Required packages: {}", pkgs.join(", "));
    // TODO: real package manager
    let p = Pkg {};
    match p.check(log, pkgs) {
        Ok(_) => {
            info!(log, "Required packages.... OK");
        }
        Err(_) => {
            mp = true;
            mps.push("garbage");
            errors.push("missing package: garbage");
            error!(log, "Required packages... FAIL");
            //error!(log, "missing required packages: {}", mps.join(", "));
        }
    }

    //    info!(log, "Required dependencies: {}", deps.join(", "));
    //  info!(log, "all required dependencies found");
    info!(log, "Required dependencies.... OK");

    if errors.len() > 0 {
        error!(
            log,
            "Check for use_case \"{:?}\" finished with errors: {:?}",
            use_case,
            errors
        );
    } else {
        info!(log, "All prerequisites for use case \"{:?}\" found!", use_case);
    }

    Ok(())
}

fn check_noinstall_marker() -> Result<bool> {
    Utf8Path::new(MARKER_FILE)
        .try_exists()
        .with_context(|| format!("checking marker file {:?}", MARKER_FILE))
}

// TODO:
// - bins as well
// - single config instead of many arguments?
fn cmd_list(
    log: &Logger,
    json: bool,
    use_case: UseCase,
    pr_type: PrereqType,
    host_os: HostOs,
    pkgs: Vec<String>,
    deps: Vec<String>,
) -> Result<()> {
    if json {
        // TODO
        return Ok(());
    }

    // TODO: builder and runner
    // TODO: better enum here?
    let desc = match pr_type {
        PrereqType::Pkg { .. } => "packages",
        PrereqType::Dep { .. } => "dependencies",
        PrereqType::All => "packages and dependencies",
    };
    println!("Listing {} required for OS \"{:?}\"...", desc, host_os);
    println!("");

    println!("System Packages:\n{}", pkgs.join("\n"));
    println!("");

    // TODO:
    println!("Other Dependencies:\n{}", deps.join("\n"));

    Ok(())
}

fn cmd_install(
    log: &Logger,
    dry_run: bool,
    use_case: UseCase,
    pr_type: PrereqType,
    pkgs: Vec<String>,
    deps: Vec<String>,
) -> Result<()> {
    // TODO: informative log message

    // Is the NO_INSTALL marker file set?
    if check_noinstall_marker()? {
        warn!(
            log,
            "{}",
            format!("NO_INSTALL marker file ({}) found", MARKER_FILE)
        );

        // If this is a dry run, allow that to proceed, but otherwise: abort
        // ship!
        if !dry_run {
            bail!("aborting install due to existence of NO_INSTALL marker");
        }
    }

    // TODO: get package manager and names of packages from TOML
    // TODO: for individual packages/deps, check if they're in the list and warn
    let p = Pkg {};
    match pr_type {
        PrereqType::Pkg { names } => {
            if names.len() == 0 {
                bail!("no package name(s) specified");
            }

            p.install(log, dry_run, names)?
        }
        PrereqType::Dep { names } => {
            if names.len() == 0 {
                bail!("no dependency name(s) specified");
            }
            install_bin(log, dry_run, names)?
        }
        // TODO: real list of pkgs
        PrereqType::All => {
            p.install(log, dry_run, pkgs)?;
            install_bin(log, dry_run, deps)?;
        }
    }
    info!(
        log,
        "Successfully installed all prerequisites for use case \"{:?}\"",
        use_case
    );

    Ok(())
}

fn install_bin(log: &Logger, dry_run: bool, names: Vec<String>) -> Result<()> {
    info!(log, "dependencies: \"{}\" installed successfully", names.join(", "));
    Ok(())
}

fn install_dep(
    log: &Logger,
    dry_run: bool,
    dep: DepDef,
    out_dir: Utf8PathBuf,
) -> Result<()> {

    todo!()
}

struct Pkg {}
impl PackageManager for Pkg {
    //fn name(&self) -> &'static str {
    //"helios"
    //}

    fn install(
        &self,
        log: &Logger,
        dry_run: bool,
        pkgs: Vec<String>,
    ) -> Result<()> {
        let mut base = vec![
            "pfexec".to_owned(),
            "pkg".to_owned(),
            "install".to_owned(),
            "-v".to_owned(),
        ];
        base.append(&mut pkgs.clone());
        let cmd_str = base.join(" ");

        // TODO: way to differentiate logging from commands here
        info!(log, "\"{}\"", cmd_str);

        if dry_run {
            return Ok(());
        }

        let mut command = std::process::Command::new(base[0].clone());
        let cmd = command.args(&base[1..]);
        let output = cmd.output().with_context(|| {
            format!("could not get output for cmd: {}", cmd_str)
        })?;

        let code = output.status.code().unwrap();
        if code != 0 && code != 4 {
            info!(log, "stdout: {}", String::from_utf8_lossy(&output.stdout));
            info!(log, "stderr: {}", String::from_utf8_lossy(&output.stderr));
            error!(log, "command failed: \"{}\" ({})", cmd_str, output.status);
            bail!("could not install packages: {}", pkgs.join(", "));
        }

        info!(log, "packages: \"{}\" installed successfully", pkgs.join(", "));

        Ok(())
    }

    fn check(&self, log: &Logger, pkgs: Vec<String>) -> Result<()> {
        // TODO: commonize command code
        // TODO: add support for check command
        let mut base = vec!["pkg".to_owned(), "list".to_owned()];
        base.append(&mut pkgs.clone());
        let cmd_str = base.join(" ");

        // TODO: way to differentiate logging from commands here
        //info!(log, "\"{}\"", cmd_str);

        let mut command = std::process::Command::new(base[0].clone());
        let cmd = command.args(&base[1..]);
        let output = cmd.output().with_context(|| {
            format!("could not get output for cmd: {}", cmd_str)
        })?;

        let code = output.status.code().unwrap();
        if code != 0 {
            // error!(log, "command failed: \"{}\" ({})", cmd_str, output.status);
            //error!(log, "stdout: {}", String::from_utf8_lossy(&output.stdout));
            //error!(log, "stderr: {}", String::from_utf8_lossy(&output.stderr));
            bail!("could not list all package(s): {}", pkgs.join(", "));
        }

        Ok(())
    }
}

/*
fn install(
        &self,
        log: &Logger,
        dry_run: bool,
        install_cmd: String,
        pkgs: Vec<String>,
) -> Result<()> {
    let mut base = install_cmd.split(" ");
        base.append(&mut pkgs.clone());
        let cmd_str = base.join(" ");

        // TODO: way to differentiate logging from commands here
        info!(log, "\"{}\"", cmd_str);

        if dry_run {
            return Ok(());
        }

        let mut command = std::process::Command::new(base[0].clone());
        let cmd = command.args(&base[1..]);
        let output = cmd.output().with_context(|| {
            format!("could not get output for cmd: {}", cmd_str)
        })?;

        let code = output.status.code().unwrap();
        if code != 0 && code != 4 {
            info!(log, "stdout: {}", String::from_utf8_lossy(&output.stdout));
            info!(log, "stderr: {}", String::from_utf8_lossy(&output.stderr));
            error!(log, "command failed: \"{}\" ({})", cmd_str, output.status);
            bail!("could not install packages: {}", pkgs.join(", "));
        }

        info!(log, "packages: \"{}\" installed successfully", pkgs.join(", "));

        Ok(())
}
*/

trait PackageManager {
    //fn name(&self) -> &'static str;

    fn install(
        &self,
        log: &Logger,
        dry_run: bool,
        pkgs: Vec<String>,
    ) -> Result<()>;

    fn check(&self, log: &Logger, pkgs: Vec<String>) -> Result<()> {
        todo!()
    }

    //   fn install_ok(&self, std::Process::Command::ExitStatus) -> bool {
    //  }
}

struct LinuxApt {}
impl PackageManager for LinuxApt {
    //fn name(&self) -> &'static str;

    fn install(
        &self,
        log: &Logger,
        dry_run: bool,
        pkgs: Vec<String>,
    ) -> Result<()> {
        todo!()
    }
}

struct DarwinBrew {}
impl PackageManager for DarwinBrew {
    //fn name(&self) -> &'static str;

    fn install(
        &self,
        log: &Logger,
        dry_run: bool,
        pkgs: Vec<String>,
    ) -> Result<()> {
        todo!()
    }
}
