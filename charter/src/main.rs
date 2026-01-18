use clap::Parser;
use std::path::PathBuf;
use anyhow::Result;
use rhythm_pi_charter::{Charter, CharterConfig, lane_assigner::LaneAssignmentStrategy, exporter::ChartFormat};

#[derive(Parser, Debug)]
#[command(author, version, about = "Audio Chart Generator for Rhythm Pi", long_about = None)]
struct Args {
    /// Path to audio file (WAV or OGG)
    #[arg(short, long)]
    audio: PathBuf,

    /// Song ID for the chart
    #[arg(short, long)]
    song_id: String,

    /// Instrument name (vocals, bass, drums, lead)
    #[arg(short, long)]
    instrument: String,

    /// Output directory for charts
    #[arg(short, long, default_value = ".")]
    output: PathBuf,

    /// BPM (if not specified, will be auto-detected)
    #[arg(long)]
    bpm: Option<f32>,

    /// Grid division (4, 8, 16, etc.)
    #[arg(long, default_value = "4")]
    grid_division: u8,

    /// Chart format (json or chart)
    #[arg(long, default_value = "json")]
    format: String,

    /// Sustain threshold for hold detection (0.0-1.0)
    #[arg(long, default_value = "0.5")]
    sustain_threshold: f32,

    /// Minimum hold duration in seconds
    #[arg(long, default_value = "0.25")]
    min_hold_duration: f32,

    /// Lane assignment strategy (sequential, frequency)
    #[arg(long, default_value = "sequential")]
    lane_strategy: String,

    /// Enable verbose logging
    #[arg(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let level = if args.verbose { "debug" } else { "info" };
    env_logger::Builder::from_default_env()
        .filter_level(level.parse()?)
        .init();

    log::info!("Starting chart generation for: {}", args.song_id);
    log::info!("Instrument: {}, Format: {}", args.instrument, args.format);

    // Validate format
    let format = ChartFormat::from_str(&args.format)
        .ok_or_else(|| anyhow::anyhow!("Invalid format: {}", args.format))?;

    // Parse lane assignment strategy
    let lane_strategy = match args.lane_strategy.to_lowercase().as_str() {
        "sequential" => LaneAssignmentStrategy::Sequential,
        "frequency" => LaneAssignmentStrategy::FrequencyBased {
            low_hz: 100.0,
            mid_hz: 500.0,
            high_hz: 2000.0,
        },
        "random" => LaneAssignmentStrategy::Random,
        s => return Err(anyhow::anyhow!("Unknown lane strategy: {}", s)),
    };

    // Create charter config
    let config = CharterConfig {
        bpm: args.bpm,
        grid_division: args.grid_division,
        sustain_threshold: args.sustain_threshold,
        min_hold_duration: args.min_hold_duration,
        lane_strategy,
    };

    let charter = Charter::new(config);

    // Generate charts for all difficulties
    log::info!("Generating charts for all difficulties...");
    let charts = charter.generate_all_difficulties(&args.audio, &args.song_id, &args.instrument)?;

    log::info!("Generated {} charts", charts.len());

    // Save charts
    for chart in &charts {
        let filename = format!(
            "{}_{}_{}",
            args.song_id.replace(" ", " "),
            args.instrument.to_lowercase(),
            chart.difficulty.to_lowercase()
        );
        let filename = format!("{}.{}", filename, format.extension());
        let output_path = args.output.join(&filename);

        chart.save(&output_path, format)?;
        log::info!(
            "Saved {} chart to: {}",
            chart.difficulty,
            output_path.display()
        );
    }

    log::info!("✓ Chart generation complete!");
    print_summary(&charts);

    Ok(())
}

fn print_summary(charts: &[rhythm_pi_charter::exporter::ChartExport]) {
    println!("\n=== Chart Summary ===");
    for chart in charts {
        println!(
            "{:<10} | {} notes | {} columns",
            chart.difficulty,
            chart.notes.len(),
            chart.columns
        );
    }
    println!("=== End Summary ===\n");
}
