use clap::Parser;
use std::io::Write as _;
use std::process;

mod output;
mod parser;

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
}

fn main() {
    let cli = Cli::parse();

    match parser::extract(&cli.path) {
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
