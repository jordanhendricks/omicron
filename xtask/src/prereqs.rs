// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! prereqs xtask: A tool to help developers understand and manage dependencies
//! needed for building and/or running Omicron.
// TODO: document position on guest OS support
// TODO: document what "running" really means

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use clap::{clap_derive::ValueEnum, Subcommand};
use serde::{Deserialize, Serialize};
use slog::{error, info, warn, Drain, Logger};

/// Marker file used to ensure this tool can't be run on shared machines (or any
/// other machine that one might want to protect against installing packages or
/// binaries).
static MARKER_FILE: &str = "/etc/opt/oxide/NO_INSTALL";

#[derive(Subcommand, Debug)]
pub(crate) enum PrereqsCmd {
    /// Check whether this system is suitable for building/running Omicron
    Check {
        /// Print output as JSON
        #[clap(short = 'j', long)]
        json: bool,

        /// Specify whether this system is for building Omicron,
        /// running/deploying it, or both
        #[clap(short = 's', long, default_value = "all")]
        system: SystemType,
    },

    /// Install prerequisite packages and dependencies
    Install {
        /// Dry run (display what commands will be run, but don't run them)
        #[clap(short = 'n')]
        dry_run: bool,

        /// Specify whether this system is for building omicron,
        /// running/deploying it, or both
        #[clap(short = 's', long, default_value = "all")]
        system: SystemType,

        /// Install a specific package/dependency.
        #[clap(subcommand)]
        dep: DepType,
    },

    // TODO: option to specify specific OS or auto-detect (I think I need a
    // default parser for that.)
    /// List prerequisite packages and dependencies
    List {
        /// Print output as JSON
        #[clap(short = 'j', long)]
        json: bool,

        /// See required packages/dependencies for building Omicron,
        /// running it, or both
        #[clap(short = 's', long, default_value = "all")]
        system: SystemType,

        /// List packages, dependencies, or both
        #[clap(subcommand)]
        dep: DepType,
    },
}

/// Whether this system is intended for building Omicron, running it, or both.
#[derive(Debug, Clone, ValueEnum)]
pub(crate) enum SystemType {
    Build,
    Deploy,
    All,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DepType {
    /// Install a specific package
    Pkg {
        /// Package name(s)
        names: Vec<String>,
    },

    /// Install a specific binary dependency
    Bin {
        /// Dependency name(s)
        names: Vec<String>,
    },

    /// Install all dependencies
    All,
}

#[derive(Debug, Serialize, Deserialize)]
//#[serde(deny_unknown_fields)]
struct PrereqsManifest {
    helios: PackageDef,
    debian_like: PackageDef,
    macos: PackageDef,
    //    bin: Vec<DepDef>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageDef {
    packages: Vec<String>,
    install_cmd: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DepDef {
    version: String,
    source: String,
    check_cmd: String,
    md5sums: Vec<(String, String)>,
}

fn read_prereq_toml(path: &Utf8Path) -> Result<PrereqsManifest> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read {:?}", path))?;
    let raw = std::str::from_utf8(&bytes).expect("config should be valid utf8");
    let cfg: PrereqsManifest = toml::from_str(raw)
        .with_context(|| format!("invalid config: {:?}", path))?;

    Ok(cfg)
}

pub(crate) fn cmd_prereqs(
    cmd: PrereqsCmd,
    host_os: Option<HostOs>,
    install_cmd: Option<String>,
) -> Result<()> {
    // Create a logger to pretty print to ttys; use bunyan-formatted output
    // otherwise.
    let log = if atty::is(atty::Stream::Stdout) {
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
    };

    // Detect OS and choose a package manager to use based on that.
    let cfg = read_prereq_toml("xtask/src/prereq_config.toml".into())?;
    // TODO: remove
    //info!(log, "config: {:?}", cfg);
    //TODO: assume helios for now
    let host_os = HostOs::Helios;
    // TODO: determine package manager from host OS + config
    let p = cfg.helios;

    match cmd {
        PrereqsCmd::Check { json, system } => todo!(),
        PrereqsCmd::Install { dry_run, system, dep } => {
            cmd_install(&log, dry_run, system, dep)?
        }
        PrereqsCmd::List { json, system, dep } => {
            cmd_list(&log, json, system, dep, host_os, p)?
        }
    }

    Ok(())
}

#[derive(Debug, Copy, Clone, ValueEnum)]
pub(crate) enum HostOs {
    Helios,
    Linux,
    Darwin,
}

struct Config<P: PackageManager> {
    os: HostOs,
    pkg_mgr: P,
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
    t: SystemType,
    dep: DepType,
    host_os: HostOs,
    p: PackageDef,
) -> Result<()> {
    if json {
        // TODO
        return Ok(());
    }

    // TODO: builder and runner
    // TODO: better enum here?
    let desc = match dep {
        DepType::Pkg { .. } => "packages",
        DepType::Bin { .. } => "dependencies",
        DepType::All => "packages and dependencies",
    };
    println!("Listing {} required for OS \"{:?}\"...", desc, host_os);
    println!("");

    println!("System Packages:\n{}", p.packages.join("\n"));
    println!("");

    // TODO:
    println!("Other Dependencies:\n{}", "");

    Ok(())
}

fn cmd_install(
    log: &Logger,
    dry_run: bool,
    t: SystemType,
    dep: DepType,
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
    let p = Pkg {};
    match dep {
        DepType::Pkg { names } => {
            if names.len() == 0 {
                // TODO: better error message here?
                bail!("no package names specified");
            }

            p.install(log, dry_run, names)?
        }
        DepType::Bin { names } => {
            if names.len() == 0 {
                // TODO: better error message here?
                bail!("no dependency names specified");
            }

            install_bin(log, dry_run, names)?
        }
        // TODO: real list of pkgs
        DepType::All => {
            p.install(log, dry_run, vec![])?;
            install_bin(log, dry_run, vec![])?
        }
    }

    Ok(())
}

fn install_bin(log: &Logger, dry_run: bool, names: Vec<String>) -> Result<()> {
    todo!()
}

struct Pkg {}
impl PackageManager for Pkg {
    fn name(&self) -> &'static str {
        "helios"
    }

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
    fn name(&self) -> &'static str;

    fn install(
        &self,
        log: &Logger,
        dry_run: bool,
        pkgs: Vec<String>,
    ) -> Result<()>;

    //   fn install_ok(&self, std::Process::Command::ExitStatus) -> bool {
    //  }
}
