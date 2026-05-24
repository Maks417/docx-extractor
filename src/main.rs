use clap::Parser;
use std::io::Write as _;
use std::process;

mod output;
mod parser;

use parser::ExtractOptions;

#[derive(Parser)]
#[command(
    name = "docx-extractor",
    about = "Extract text and images from DOCX files as JSON",
    version
)]
struct Cli {
    /// Path to the .docx file
    path: String,

    /// Pretty-print JSON output
    #[arg(short, long)]
    pretty: bool,

    /// Write JSON to this file instead of stdout
    #[arg(short, long)]
    output: Option<String>,

    /// Skip extraction of base64-encoded image data (image references on
    /// sections are still preserved; the top-level `images` array is empty)
    #[arg(long)]
    no_images: bool,

    /// Maximum size of an individual embedded image, in bytes. Images larger
    /// than this are skipped with a stderr warning. Default: 10485760 (10 MB).
    #[arg(long, value_name = "BYTES")]
    max_image_bytes: Option<u64>,
}

fn main() {
    let cli = Cli::parse();

    let defaults = ExtractOptions::default();
    let opts = ExtractOptions {
        include_images: !cli.no_images,
        max_image_bytes: cli.max_image_bytes.unwrap_or(defaults.max_image_bytes),
    };

    match parser::extract(&cli.path, &opts) {
        Ok(doc) => {
            let json = if cli.pretty {
                serde_json::to_string_pretty(&doc)
            } else {
                serde_json::to_string(&doc)
            };
            match json {
                Ok(s) => {
                    if let Some(ref path) = cli.output {
                        match std::fs::File::create(path) {
                            Ok(mut f) => {
                                if let Err(e) = f.write_all(s.as_bytes()) {
                                    eprintln!("Failed to write output file: {e}");
                                    process::exit(1);
                                }
                            }
                            Err(e) => {
                                eprintln!("Failed to create output file: {e}");
                                process::exit(1);
                            }
                        }
                    } else {
                        println!("{s}");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize output: {e}");
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            process::exit(1);
        }
    }
}
