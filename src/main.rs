mod adapters;
mod config;
mod parser;
mod runner;
mod storage;
mod summary;
mod upgrade;
#[allow(dead_code)]
mod version_shape;

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use regex::Regex;

use crate::parser::{Collector, StreamParser};
use crate::storage::Meta;

#[derive(Parser)]
#[command(
    name = "gatr",
    version = env!("GATR_VERSION"),
    about = "GATe Runner — run a verification gate once, keep the full log, print a compact machine-stable summary"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run a command, tee the full log to disk, print the GATR summary
    Run(RunArgs),
    /// Reprint the most recent summary without rerunning
    Last {
        /// Only consider runs with this tag
        #[arg(long)]
        tag: Option<String>,
        /// Resolve another repo's state without cd'ing into it
        #[arg(long)]
        project: Option<PathBuf>,
        /// Print the raw .meta.json instead of the human summary
        #[arg(long)]
        json: bool,
    },
    /// Print the extracted error blocks from the most recent log
    Errors {
        /// Only consider runs with this tag
        #[arg(long)]
        tag: Option<String>,
        /// Re-scan the full log for every match (beyond the stored blocks)
        #[arg(long)]
        all: bool,
    },
    /// Print the path of the most recent log (or its content with --cat)
    Log {
        /// Only consider runs with this tag
        #[arg(long)]
        tag: Option<String>,
        /// Print the log content instead of its path
        #[arg(long)]
        cat: bool,
    },
    /// Prune logs beyond the retention window (20 per tag per project)
    Gc {
        /// Prune every project's state, not just the current one
        #[arg(long)]
        all: bool,
    },
    /// Self-update: pull the source checkout, rebuild, replace this binary
    Upgrade,
}

#[derive(Args)]
struct RunArgs {
    /// Name of the gate (ci, test, clippy, …); default: first word of the command
    #[arg(long)]
    tag: Option<String>,
    /// Error-pattern set: auto, cargo, tsc, pytest, jest, eslint, generic, or a .gatr.toml adapter
    #[arg(long, default_value = "auto")]
    adapter: String,
    /// Drop matching lines from the display (never from the log); repeatable
    #[arg(long = "filter")]
    filters: Vec<String>,
    /// Raw lines of tail to include in the summary
    #[arg(long, default_value_t = 10)]
    tail: usize,
    /// Max error blocks to print in the summary
    #[arg(long = "errors", default_value_t = 3)]
    max_errors: usize,
    /// Print the contract line only
    #[arg(long)]
    quiet: bool,
    /// Print the machine summary as JSON instead of the human output
    #[arg(long)]
    json: bool,
    /// Kill the command after this long (90s, 10m, 1h30m) and report exit=124
    #[arg(long)]
    timeout: Option<String>,
    /// The command to run (after --)
    #[arg(last = true, required = true, value_name = "CMD")]
    command: Vec<String>,
}

fn main() {
    let cli = Cli::parse();
    let code = match dispatch(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("gatr: {err:#}");
            1
        }
    };
    let _ = std::io::stdout().flush();
    std::process::exit(code);
}

fn dispatch(cli: Cli) -> Result<i32> {
    match cli.cmd {
        Cmd::Run(args) => cmd_run(&args),
        Cmd::Last { tag, project, json } => cmd_last(tag.as_deref(), project.as_deref(), json),
        Cmd::Errors { tag, all } => cmd_errors(tag.as_deref(), all),
        Cmd::Log { tag, cat } => cmd_log(tag.as_deref(), cat),
        Cmd::Gc { all } => cmd_gc(all),
        Cmd::Upgrade => upgrade::cmd_upgrade(),
    }
}

fn cwd() -> Result<PathBuf> {
    std::env::current_dir().context("cannot determine current directory")
}

fn cmd_run(args: &RunArgs) -> Result<i32> {
    let cwd = cwd()?;
    let project_root = storage::project_root(&cwd);
    let cfg = config::load(&project_root)?;

    let tag = args.tag.clone().unwrap_or_else(|| {
        Path::new(&args.command[0]).file_stem().map_or_else(
            || args.command[0].clone(),
            |s| s.to_string_lossy().to_string(),
        )
    });

    // Adapter resolution: explicit flag > [tags.<tag>] config > argv sniff > content sniff.
    let tag_cfg = cfg.tags.get(&tag);
    let named_adapter = if args.adapter != "auto" {
        Some(args.adapter.clone())
    } else if let Some(a) = tag_cfg.and_then(|t| t.adapter.clone()) {
        Some(a)
    } else {
        adapters::sniff_from_argv(&args.command).map(String::from)
    };
    let (collectors, auto) = match &named_adapter {
        Some(name) => (
            vec![Collector::new(adapters::resolve(name, &cfg.adapters)?)],
            false,
        ),
        None => {
            let mut cs: Vec<Collector> = adapters::CANDIDATES
                .iter()
                .map(|n| Collector::new(adapters::built_in(n).expect("built-in")))
                .collect();
            cs.push(Collector::new(
                adapters::built_in("generic").expect("built-in"),
            ));
            (cs, true)
        }
    };

    let mut filter_patterns: Vec<String> = cfg.run.filters.clone();
    if let Some(t) = tag_cfg {
        filter_patterns.extend(t.filters.clone());
    }
    filter_patterns.extend(args.filters.clone());
    let filters: Vec<Regex> = filter_patterns
        .iter()
        .map(|p| Regex::new(p).with_context(|| format!("invalid --filter regex '{p}'")))
        .collect::<Result<_>>()?;

    let timeout = args
        .timeout
        .as_deref()
        .map(runner::parse_timeout)
        .transpose()?;

    let state_dir = storage::project_state_dir(&cwd);
    std::fs::create_dir_all(&state_dir)
        .with_context(|| format!("failed to create state dir {}", state_dir.display()))?;
    let started = chrono::Utc::now().format("%Y-%m-%dT%H-%M-%S").to_string();
    let log_path = storage::new_log_path(&state_dir, &started, &tag);

    let parser = StreamParser::new(collectors, auto, args.tail, filters);
    let result = runner::run_command(&args.command, &log_path, parser, timeout)?;

    let meta = Meta {
        gatr_meta: storage::META_SCHEMA,
        cmd: summary::join_cmd(&args.command),
        tag,
        adapter: result.parse.adapter.clone(),
        exit: result.exit,
        dur_s: (result.dur_s * 10.0).round() / 10.0,
        errors: result.parse.errors,
        warnings: result.parse.warnings,
        started: format!("{started}Z"),
        project_path: project_root.to_string_lossy().to_string(),
        log: log_path.to_string_lossy().to_string(),
        error_blocks: result.parse.blocks,
        tail: result.parse.tail,
    };
    storage::write_meta(&meta, &log_path)?;
    storage::prune(&state_dir);

    if args.json {
        println!("{}", serde_json::to_string(&meta)?);
    } else if args.quiet {
        println!("{}", summary::contract_line(&meta));
    } else {
        println!("{}", summary::render_human(&meta, args.max_errors));
        if result.timed_out {
            eprintln!(
                "gatr: command timed out after {}",
                args.timeout.as_deref().unwrap_or("?")
            );
        }
    }
    // Exit code passthrough is non-negotiable: gatr is a drop-in wrapper.
    Ok(meta.exit)
}

fn latest_for(dir: &Path, tag: Option<&str>, what: &str) -> Result<(PathBuf, Meta)> {
    storage::latest(dir, tag).ok_or_else(|| {
        let scope = tag.map_or(String::new(), |t| format!(" with tag '{t}'"));
        anyhow::anyhow!("no recorded {what}{scope} — run `gatr run -- <cmd>` first")
    })
}

fn cmd_last(tag: Option<&str>, project: Option<&Path>, json: bool) -> Result<i32> {
    let anchor = match project {
        Some(p) => std::fs::canonicalize(p)
            .with_context(|| format!("--project path not found: {}", p.display()))?,
        None => cwd()?,
    };
    let dir = storage::project_state_dir(&anchor);
    let (_, meta) = latest_for(&dir, tag, "runs")?;
    if json {
        println!("{}", serde_json::to_string(&meta)?);
    } else {
        println!("{}", summary::render_human(&meta, 3));
    }
    Ok(0)
}

fn cmd_errors(tag: Option<&str>, all: bool) -> Result<i32> {
    let cwd = cwd()?;
    let dir = storage::project_state_dir(&cwd);
    let (meta_path, meta) = latest_for(&dir, tag, "runs")?;
    let blocks = if all {
        reparse_blocks(&meta_path, &meta)?
    } else {
        meta.error_blocks.clone()
    };
    if blocks.is_empty() {
        println!(
            "no error blocks in the last {} run (exit={})",
            meta.tag, meta.exit
        );
        return Ok(0);
    }
    for (i, block) in blocks.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("── error {}/{} ──", i + 1, blocks.len());
        println!("{block}");
    }
    Ok(0)
}

/// Re-scan the stored log with the recorded adapter to surface every match,
/// not just the blocks captured within the streaming bound.
fn reparse_blocks(meta_path: &Path, meta: &Meta) -> Result<Vec<String>> {
    let log_path = sibling_log(meta_path, meta);
    let cfg = config::load(Path::new(&meta.project_path)).unwrap_or_default();
    let adapter = adapters::resolve(&meta.adapter, &cfg.adapters)?;
    let text = std::fs::read_to_string(&log_path)
        .with_context(|| format!("failed to read log {}", log_path.display()))?;
    let mut all_blocks = Vec::new();
    let mut collector = Collector::new(adapter);
    for line in text.lines() {
        collector.feed(&parser::strip_ansi(line));
        if collector.blocks.len() == parser::MAX_BLOCKS {
            all_blocks.append(&mut collector.blocks);
        }
    }
    collector.finish();
    all_blocks.append(&mut collector.blocks);
    Ok(all_blocks)
}

/// Prefer the recorded absolute log path; fall back to the meta's sibling if
/// the state dir has been moved.
fn sibling_log(meta_path: &Path, meta: &Meta) -> PathBuf {
    let recorded = PathBuf::from(&meta.log);
    if recorded.exists() {
        recorded
    } else {
        meta_path.with_extension("").with_extension("log")
    }
}

fn cmd_log(tag: Option<&str>, cat: bool) -> Result<i32> {
    let cwd = cwd()?;
    let dir = storage::project_state_dir(&cwd);
    let (meta_path, meta) = latest_for(&dir, tag, "logs")?;
    let log_path = sibling_log(&meta_path, &meta);
    if cat {
        let file = std::fs::File::open(&log_path)
            .with_context(|| format!("failed to open {}", log_path.display()))?;
        let mut reader = std::io::BufReader::new(file);
        std::io::copy(&mut reader, &mut std::io::stdout())
            .context("failed to stream log to stdout")?;
    } else {
        println!("{}", log_path.display());
    }
    Ok(0)
}

fn cmd_gc(all: bool) -> Result<i32> {
    let mut removed = 0;
    if all {
        let root = storage::state_root();
        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.filter_map(std::result::Result::ok) {
                if entry.path().is_dir() {
                    removed += storage::prune(&entry.path());
                }
            }
        }
    } else {
        removed += storage::prune(&storage::project_state_dir(&cwd()?));
    }
    println!(
        "gatr gc: removed {removed} runs beyond retention ({} per tag)",
        storage::RETAIN_PER_TAG
    );
    Ok(0)
}
