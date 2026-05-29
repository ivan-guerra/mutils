//! Command-line tool for simulating human-like mouse movements using recorded segment data.

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use enigo::{Enigo, Mouse, Settings};

use mousesim::{SegmentDatabase, move_mouse_humanlike};

/// A mouse movement simulator that generates human-like mouse movements
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to directory containing segment recordings
    segments_dir: PathBuf,

    /// Set logging level (off, error, warn, info, debug, trace)
    #[arg(long, value_name = "LEVEL", default_value = "off")]
    log_level: simplelog::LevelFilter,
}

#[doc(hidden)]
fn main() -> Result<()> {
    let args = Args::parse();

    if args.log_level != simplelog::LevelFilter::Off {
        simplelog::TermLogger::init(
            args.log_level,
            simplelog::ConfigBuilder::new()
                .set_thread_level(simplelog::LevelFilter::Off)
                .add_filter_allow_str("mousesim")
                .add_filter_allow_str("segments")
                .build(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        )
        .context("Failed to initialize logger")?;
    }

    // Load the segment database
    println!(
        "Loading segment database from {}...",
        args.segments_dir.display()
    );
    let segdb = SegmentDatabase::load_from_directory(&args.segments_dir)
        .context("Failed to load segment database")?;
    println!("Loaded {} segments", segdb.size());

    // Initialize enigo for getting current mouse position
    let enigo = Enigo::new(&Settings::default())
        .map_err(|e| anyhow::anyhow!("Failed to initialize mouse controller: {}", e))?;

    println!("\nMouse Movement Simulator");
    println!("========================");
    println!("Enter target coordinates as 'X Y' (e.g., '500 300')");
    println!("Type 'quit' or 'exit' to stop\n");

    let stdin = io::stdin();
    let mut input = String::new();

    loop {
        print!("> ");
        io::stdout().flush()?;

        input.clear();
        stdin.read_line(&mut input)?;

        let trimmed = input.trim();

        // Check for exit commands
        if trimmed.eq_ignore_ascii_case("quit") || trimmed.eq_ignore_ascii_case("exit") {
            println!("Exiting...");
            break;
        }

        // Parse coordinates
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() != 2 {
            eprintln!("Error: Please enter two numbers separated by space (X Y)");
            continue;
        }

        let target_x: i32 = match parts[0].parse() {
            Ok(x) => x,
            Err(_) => {
                eprintln!("Error: Invalid X coordinate '{}'", parts[0]);
                continue;
            }
        };
        let target_y: i32 = match parts[1].parse() {
            Ok(y) => y,
            Err(_) => {
                eprintln!("Error: Invalid Y coordinate '{}'", parts[1]);
                continue;
            }
        };

        // Get current mouse position
        let (current_x, current_y) = enigo.location().context("Failed to get mouse position")?;

        println!(
            "Moving from ({}, {}) to ({}, {})",
            current_x, current_y, target_x, target_y
        );

        // Perform the movement
        if let Err(e) = move_mouse_humanlike(&segdb, (current_x, current_y), (target_x, target_y)) {
            eprintln!("Error moving mouse: {}", e);
        }
    }

    Ok(())
}
