use std::io::Write;
use std::process;
use std::env;
use std::path::PathBuf;
use std::io::Read;
use std::fs::{self, File, OpenOptions};
use std::io::BufReader;
use std::str::from_utf8;

use betwixt_parse::TangleMode;
use betwixt_parse::{BETWIXT_TOKEN, CLOSE_TOKEN, Document, MarkdownParsers, betwixt, code, section};
use clap::Parser;

#[derive(Parser)]
#[command(name = "betwixt")]
#[command(author, version, about)]
struct Cli {
	file: PathBuf,
	#[arg(short = 'o', long = "outpath")]
	output_dir: Option<PathBuf>,
	#[arg(long = "no-strict")]
	no_strict: bool,
	#[arg(short = 't')]
	tag: Option<String>
}

fn main() {
	let cli = Cli::parse();

	let out_dir = match cli.output_dir {
		Some(dir) => dir,
		None => env::current_dir().expect("betwixt must be in a directory, or you must specify --outpath")
	};
	let dir_meta = fs::metadata(&out_dir).expect("output directory doesn't exist");
	if !dir_meta.is_dir() {
		println!("output directory {} is not a directory", out_dir.to_string_lossy());
		process::exit(1);
	}
	let file = File::open(cli.file).expect("failed to open specified input FILE");
	let mut reader = BufReader::new(file);
	let mut bytes = Vec::new();
	reader.read_to_end(&mut bytes).expect("failed reading contents of file");

	// TODO handle flavors... and this kinda sucks so rework this
	let mut parsers = MarkdownParsers{
		code: code("```", "```"),
        section: section('#'),
        betwixt: betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
	};
	
	let markdown = Document::from_contents(&bytes[..], &mut parsers);
	for block in markdown.code_blocks.iter() {
		if let Some(filter) = cli.tag.as_ref() {
			match block.properties.tag {
				Some(tag) => {
					if from_utf8(tag).unwrap() != filter {
						continue
					}
				},
				None => continue
			}
		}
		// FIXME don't repeatedly open and write files. Do it once. This is easier for now
		// FIXME don't just use utf8 blindly on filenames
		if let Some(mode) = &block.properties.mode {
			if let Some(filename) = block.properties.filename {
				let mut file = match mode {
					TangleMode::Overwrite => {
						let mut path = out_dir.clone();
						path.push(from_utf8(filename).unwrap());
						OpenOptions::new().create(true).write(true).truncate(true).open(path).unwrap()
					},
					TangleMode::Append => {
						let mut path = out_dir.clone();
						path.push(from_utf8(filename).unwrap());
						OpenOptions::new().write(true).append(true).open(path).unwrap()
					},
					TangleMode::Prepend => {
						panic!("prepend mode is unimplemented");
					},
					TangleMode::Insert(_) => {
						panic!("insert mode is unimplemented");
					}
				};
				file.write_all(block.part.contents).expect("error writing to file");
			} else {
				if !cli.no_strict {
					panic!("code file without mode or filename found, strict mode enforced")
				}
				continue;
			}
		} else {
			if !cli.no_strict {
				panic!("code file without mode or filename found, strict mode enforced")
			}
			continue;
		};
	}
}

