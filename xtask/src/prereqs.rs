use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use camino::Utf8Path;
use clap::{clap_derive::ValueEnum, Subcommand};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use slog::{error, info, warn, Drain, Logger};

static MARKER_FILE: &str = "/etc/opt/oxide/NO_INSTALL";

#[derive(Subcommand)]
pub(crate) enum PrereqCmd {
    /// Check whether this system is suitable for building/running omicron
    Check {
        /// Print output as JSON
        #[clap(short = 'j', long)]
        json: bool,

        /// Specify whether this system is for building omicron,
        /// running/deploying it, or both
        #[clap(short = 's', long, default_value = "all")]
        system: SystemType,
    },

    /// Install prerequisite packages and dependencies
    // TODO: option for -y?
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

    /// List prerequisite packages and dependencies
    // TODO: option to specify specific OS
    List,
}

#[derive(Debug, Clone, ValueEnum)]
pub(crate) enum SystemType {
    Build,
    Deploy,
    All,
}

#[derive(Subcommand)]
pub(crate) enum DepType {
    /// Install a specific package
    Pkg {
        /// Dry run (display what commands will be run, but don't run them)
        #[clap(short = 'n')]
        dry_run: bool,

        /// Package name
        name: String,
    },

    /// Install a specific binary dependency
    Bin {
        /// Dry run (display what commands will be run, but don't run them)
        #[clap(short = 'n')]
        dry_run: bool,

        /// Dependency name
        name: String,
    },

    /// Install all dependencies
    All {
        /// Dry run (display what commands will be run, but don't run them)
        #[clap(short = 'n')]
        dry_run: bool,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct PrereqManifest {
    packages: PackageDef,
}

#[derive(Debug, Serialize, Deserialize)]
struct PackageDef {
    helios: Vec<PathBuf>,
}

fn read_cargo_toml(path: &Utf8Path) -> Result<PrereqManifest> {
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
        let drain =
            std::sync::Mutex::new(slog_bunyan::with_name("omicron-prereqs", std::io::stdout())
                .build())
                .fuse();
        slog::Logger::root(drain, slog::o!())
    };

    //println!("{:?}", read_cargo_toml("xtask/src/prereq_config.toml".into())?);

    match cmd {
        PrereqCmd::Check { json, system } => todo!(),
        PrereqCmd::Install { dry_run, system, dep } => {
            cmd_install(&log, dry_run, system, dep);
        }
        PrereqCmd::List => todo!(),
    }

    Ok(())
}

fn cmd_install(
    log: &Logger,
    dry_run: bool,
    t: SystemType,
    dep: DepType,
) -> Result<()> {
    let no_install = Utf8Path::new(MARKER_FILE)
        .try_exists()
        .with_context(|| format!("checking marker file {:?}", MARKER_FILE))?;

    // TODO: informative log message

    match (no_install, dry_run) {
        (true, true) => {
            // Since this is a dry run, allow things to proceed, but make some
            // noise about the marker file being found.
            warn!(log, "NO_INSTALL marker file found");
        }
        (true, false) => {
            // Not a dry run, so bail out here.
            bail!("NO_INSTALL marker file found; aborting install");
        }
        _ => {}
    }

    let p = Pkg {};
    match dep {
        DepType::Pkg { dry_run, name } => {
            p.install(log, dry_run, vec![name])?
        }
        DepType::Bin { dry_run, name } => install_bin(log, dry_run, name)?,
        // TODO: real list of pkgs
        DepType::All { dry_run } => install_all(log, dry_run, vec![])?,
    }

    Ok(())
}

fn install_all(log: &Logger, dry_run: bool, pkgs: Vec<String>) -> Result<()> {
    todo!()
}

fn install_bin(log: &Logger, dry_run: bool, name: String) -> Result<()> {
    todo!()
}

struct Pkg {}
impl PackageManager for Pkg {
    /*
    fn install_pkgs(&self, log: &Logger, dry_run: bool, pkgs: Vec<String>) -> Result<()> {


        let cmd_str = ["pfexec pkg install -v ", name].join(" ");
        let mut command = std::process::Command::new("pfexec");
        let cmd = command.args(["pkg", "install", "-v"]).arg(name);

        let mut args = Vec::new();
        args.push(cmd.get_program());
        let mut args = cmd.get_args().map(|s| args.push(s));
        let s = args.collect::<Vec<&std::ffi::OsStr>>().join(std::ffi::OsStr::new(" "));
        info!(log, "{:?} {:?}", cmd.get_program(), args);
        if dry_run {
            return Ok(());
        }

        let output = cmd.output().context("could not get cmd output")?;

        if !output.status.success() {
            bail!("command failed");
        }

        Ok(())
    }
    */

    fn name(&self) -> &'static str {
        "helios"
    }

    fn install(
        &self,
        log: &Logger,
        dry_run: bool,
        mut pkgs: Vec<String>,
    ) -> Result<()> {
        let mut base = vec![
            "pfexec".to_owned(),
            "pkg".to_owned(),
            "install".to_owned(),
            "-v".to_owned(),
        ];
        base.append(&mut pkgs);
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
            error!(
                log,
                "could not install packages with command: \"{}\" ({})",
                cmd_str,
                output.status
            );
        }

        info!(log, "packages installed successfully");

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
