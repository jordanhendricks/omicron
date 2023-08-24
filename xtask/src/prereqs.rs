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
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use slog::{error, info, warn, Drain, Logger};

/// Marker file used to ensure this tool can't be run on shared machines (or any
/// other machine that one might want to protect against installing packages or
/// binaries).
static MARKER_FILE: &str = "/etc/opt/oxide/NO_INSTALL";

#[derive(Subcommand, Debug)]
pub(crate) enum PrereqCmd {
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
    List,
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
struct PrereqManifest {
    packages: PackageDef,
}

#[derive(Debug, Serialize, Deserialize)]
struct PackageDef {
    helios: Vec<PathBuf>,
}

fn read_prereq_toml(path: &Utf8Path) -> Result<PrereqManifest> {
    let bytes =
        std::fs::read(path).with_context(|| format!("read {:?}", path))?;
    let raw = std::str::from_utf8(&bytes).expect("config should be valid utf8");
    let cfg: PrereqManifest = toml::from_str(raw).expect("config is parseable");

    Ok(cfg)
}

pub(crate) fn cmd_prereqs(cmd: PrereqCmd) -> Result<()> {
    // Pretty print to ttys; use bunyan-formatted output otherwise.
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

    //println!("{:?}", read_prereq_toml("xtask/src/prereq_config.toml".into())?);
    //info!(log, "{:?}", cmd);

    match cmd {
        PrereqCmd::Check { json, system } => todo!(),
        PrereqCmd::Install { dry_run, system, dep } => {
            cmd_install(&log, dry_run, system, dep)?
        }
        PrereqCmd::List => todo!(),
    }

    Ok(())
}

fn check_noinstall_marker() -> Result<bool> {
    Utf8Path::new(MARKER_FILE)
        .try_exists()
        .with_context(|| format!("checking marker file {:?}", MARKER_FILE))
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

trait PackageManager {
    fn name(&self) -> &'static str;

    fn install(
        &self,
        log: &Logger,
        dry_run: bool,
        pkgs: Vec<String>,
    ) -> Result<()>;
}
