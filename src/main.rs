use anyhow::Result;
use clap::Parser as ClapParser;
use colored::Colorize;
use pointless_pointer::PointlessPointer;
use std::path::PathBuf;

#[derive(ClapParser, Debug)]
#[command(name = "pointless_pointer")]
#[command(about = "Detect pointless overrides in Helm values files")]
struct Args {
    /// Base values file
    base: PathBuf,

    /// Override files (can be specified multiple times with -f)
    #[arg(short = 'f', long = "file", value_name = "FILE")]
    overrides: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let analyzer = PointlessPointer::new(args.base, args.overrides);
    let (pointless_overrides, warnings) = analyzer.analyze()?;

    // Report warnings first
    if !warnings.is_empty() {
        println!(
            "{}",
            "⚠ Warnings - Duplicate keys with different values in the same document:".yellow()
        );
        println!(
            "  {} Consider keeping only one",
            "Suggestion:".bold().blue()
        );
        println!();

        for warning in &warnings {
            print!("{warning}");
            println!();
        }

        println!(
            "{} {} duplicate key warning(s)",
            "Warning summary:".bold(),
            warnings.len().to_string().yellow()
        );
        println!();
    }

    // Report pointless overrides
    if pointless_overrides.is_empty() {
        if warnings.is_empty() {
            println!("{}", "✓ No pointless overrides found!".green());
        } else {
            println!(
                "{}",
                "✓ No pointless overrides found (but see warnings above)".green()
            );
        }
    } else {
        println!("{}", "⚠ Found pointless overrides:".yellow());
        println!();

        for override_item in &pointless_overrides {
            print!("{override_item}");
            println!();
        }

        println!(
            "{} {} pointless override(s) found",
            "Summary:".bold(),
            pointless_overrides.len().to_string().red()
        );
    }

    Ok(())
}
