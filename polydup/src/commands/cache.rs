//! Cache management commands

use anyhow::{Context, Result};
use colored::Colorize;
use dupe_core::{HashCache, Scanner};
use std::path::PathBuf;

use crate::config;

/// Cache subcommands
#[derive(Debug, clap::Parser)]
pub enum CacheCommand {
    /// Build hash cache for faster scanning
    Build {
        /// Paths to scan (defaults to current directory)
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Output path for cache file
        #[arg(long, short = 'o', default_value = ".polydup-cache.json")]
        output: PathBuf,

        /// Verbose output
        #[arg(long, short = 'v')]
        verbose: bool,
    },

    /// Show cache statistics
    Info {
        /// Path to cache file
        #[arg(long, default_value = ".polydup-cache.json")]
        cache: PathBuf,
    },

    /// Clear/delete the cache
    Clear {
        /// Path to cache file
        #[arg(long, default_value = ".polydup-cache.json")]
        cache: PathBuf,
    },
}

impl CacheCommand {
    pub fn execute(self) -> Result<()> {
        match self {
            CacheCommand::Build {
                paths,
                output,
                verbose,
            } => build_cache(paths, output, verbose),
            CacheCommand::Info { cache } => show_cache_info(cache),
            CacheCommand::Clear { cache } => clear_cache(cache),
        }
    }
}

fn build_cache(paths: Vec<PathBuf>, output: PathBuf, verbose: bool) -> Result<()> {
    // Load config from .polyduprc.toml and use its values
    // (Cache command doesn't accept CLI threshold args to avoid confusion with global args)
    let config = config::Config::load().ok().flatten().unwrap_or_default();

    let min_block_size = config.scan.min_block_size;
    let similarity = config.scan.similarity_threshold;

    if verbose {
        eprintln!("{}", "Building hash cache...".cyan().bold());
        eprintln!("  Paths: {:?}", paths);
        eprintln!("  Output: {}", output.display());
        eprintln!("  Threshold: {} tokens", min_block_size);
        eprintln!("  Similarity: {:.1}%", similarity * 100.0);
        eprintln!();
    }

    // Create scanner
    let scanner = Scanner::with_config(min_block_size, similarity)?;

    // Build cache
    let start = std::time::Instant::now();
    let cache = scanner
        .build_cache(paths)
        .context("Failed to build hash cache")?;
    let duration = start.elapsed();

    // Show statistics
    let stats = cache.stats();
    if verbose {
        eprintln!("{}", "Cache Statistics:".green().bold());
        eprintln!("  Files cached: {}", stats.files_cached);
        eprintln!("  Total hashes: {}", stats.total_hashes);
        eprintln!("  Total locations: {}", stats.total_locations);
        if let Some(ref commit) = stats.git_commit {
            eprintln!("  Git commit: {}", commit);
        }
        eprintln!("  Build time: {:.2}s", duration.as_secs_f64());
        eprintln!();
    }

    // Save cache
    cache
        .save(&output)
        .with_context(|| format!("Failed to save cache to {}", output.display()))?;

    println!(
        "{}",
        format!(
            "✓ Cache built successfully: {} hashes from {} files",
            stats.total_hashes, stats.files_cached
        )
        .green()
        .bold()
    );

    println!("{}", format!("  Saved to: {}", output.display()).dimmed());
    println!(
        "{}",
        format!("  Build time: {:.2}s", duration.as_secs_f64()).dimmed()
    );

    println!();
    println!(
        "{}",
        "Tip: Use 'polydup scan --git-diff <range>' for fast PR checks".dimmed()
    );

    Ok(())
}

fn show_cache_info(cache_path: PathBuf) -> Result<()> {
    if !cache_path.exists() {
        anyhow::bail!(
            "Cache file not found: {}\nRun 'polydup cache build' to create it.",
            cache_path.display()
        );
    }

    let cache = HashCache::load(&cache_path)
        .with_context(|| format!("Failed to load cache from {}", cache_path.display()))?;

    let stats = cache.stats();

    println!("{}", "Cache Information".cyan().bold());
    println!("{}", "═".repeat(50).dimmed());
    println!();

    println!("{}: {}", "Cache file".bold(), cache_path.display());
    println!("{}: v{}", "Version".bold(), cache.version);
    println!();

    println!("{}", "Configuration:".bold());
    println!("  Min tokens:       {}", cache.min_block_size);
    println!();

    println!("{}", "Statistics:".bold());
    println!("  Files cached:     {}", stats.files_cached);
    println!("  Unique hashes:    {}", stats.total_hashes);
    println!("  Total locations:  {}", stats.total_locations);
    println!();

    if let Some(ref commit) = stats.git_commit {
        println!("{}: {}", "Git commit".bold(), commit);
    }

    let created_time = chrono::DateTime::from_timestamp(stats.created_at as i64, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    println!("{}: {}", "Created at".bold(), created_time);

    println!();
    println!(
        "{}",
        "Tip: Cache enables 10-100x faster git-diff scans".dimmed()
    );

    Ok(())
}

fn clear_cache(cache_path: PathBuf) -> Result<()> {
    if !cache_path.exists() {
        println!(
            "{}",
            format!("Cache file does not exist: {}", cache_path.display()).yellow()
        );
        return Ok(());
    }

    std::fs::remove_file(&cache_path)
        .with_context(|| format!("Failed to delete cache file: {}", cache_path.display()))?;

    println!(
        "{}",
        format!("✓ Cache cleared: {}", cache_path.display())
            .green()
            .bold()
    );

    Ok(())
}
