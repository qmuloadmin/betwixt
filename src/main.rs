use std::collections::HashSet;
use std::env;
use std::fmt::Display;
use std::fs::{self, File, OpenOptions};
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::process;
use std::str::from_utf8;

use anyhow::{anyhow, Context, Result};
use betwixt_parse::TangleMode;
use betwixt_parse::{
    betwixt, code, section, Code, Document, MarkdownParsers, BETWIXT_TOKEN, CLOSE_TOKEN,
};
use clap::{Parser, ValueEnum};

#[derive(ValueEnum, Clone)]
enum Flavor {
    // markdown used by github and many others
    Github,
    // markdown flavor to use when extracting markdown from markdown code blocks
    //
    // particularly useful for eating your own dogfood and turning betwixt's documents
    // into betwixt's tests
    Nested,
}

impl Display for Flavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Flavor::Github => "github",
                Flavor::Nested => "nested",
            }
        )
    }
}

#[derive(Parser)]
#[command(name = "betwixt")]
#[command(author, version, about)]
struct Cli {
    file: PathBuf,
    #[arg(short = 'o', long = "outpath")]
    /// The root directory to write all files to
    output_dir: Option<PathBuf>,
    #[arg(long = "no-strict")]
    /// Ignore certain errors that are probably a bad thing
    no_strict: bool,
    #[arg(short = 't')]
    /// Only Tangle blocks with this tag
    tag: Option<String>,
    #[arg(long = "flavor", default_value_t = Flavor::Github)]
    /// The markdown flavor to use for parsing (usually ignore this)
    flavor: Flavor,
    #[arg(short = 'e')]
    /// A list of block IDs that should be executed in addition to being tangled
    execute: Option<Vec<String>>,
}

fn execute(block: &Code, exec_ids: &HashSet<String>) -> Result<Option<String>> {
    if let Some(id) = &block.part.id {
        let id = from_utf8(&id).unwrap();
        if exec_ids.contains(id) {
            let cmd = block
                .properties
                .cmd
                .context(format!("specified exec id {} has no cmd specified", id))?;
            let cmd = from_utf8(cmd).unwrap();
            let cmds = cmd.split("&&").into_iter();
            let mut output: Vec<u8> = Vec::new();
            for cmd in cmds {
                let cmd: Vec<&str> = cmd.split_whitespace().collect();
                let mut command = std::process::Command::new(cmd[0]);
                output = command
                    .args(&cmd[1..cmd.len()])
                    .output()
                    .context(format!("failed executing command for id {}", id))?
                    .stdout;
            }
            Ok(Some(from_utf8(&output).unwrap().to_owned()))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

fn tangle(cli: Cli) -> Result<()> {
    let exec_ids = match cli.execute {
        Some(ids) => ids.into_iter().collect(),
        None => HashSet::new(),
    };
    let out_dir = cli.output_dir.unwrap_or(
        env::current_dir().context("betwixt must be in a directory or must specify --output")?,
    );
    let dir_meta = fs::metadata(&out_dir).context("output directory does not exist")?;
    if !dir_meta.is_dir() {
        return Err(anyhow!(
            "output directory {} is not a directory",
            out_dir.to_string_lossy()
        ));
    };
    let file = File::open(cli.file).context("unable to open input file")?;
    std::env::set_current_dir(&out_dir).context("unable to change to output directory")?;

    let mut reader = BufReader::new(file);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .context("failed reading contents of file")?;

    let parsers = match cli.flavor {
        Flavor::Github => MarkdownParsers {
            code: code("```", "```"),
            section: section('#'),
            betwixt: betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
            strict: !cli.no_strict,
        },
        Flavor::Nested => MarkdownParsers {
            code: code("'''", "'''"),
            section: section('#'),
            betwixt: betwixt(BETWIXT_TOKEN, CLOSE_TOKEN),
            strict: !cli.no_strict,
        },
    };
    let markdown =
        Document::from_contents(&bytes[..], parsers).context("strict mode: failed to parse")?;
    for block in markdown.code_blocks.iter() {
        if let Some(filter) = cli.tag.as_ref() {
            match block.properties.tag {
                Some(tag) => {
                    if from_utf8(tag).context("failed to parse tag as utf8")? != filter {
                        continue;
                    }
                }
                None => continue,
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
                        OpenOptions::new()
                            .create(true)
                            .write(true)
                            .truncate(true)
                            .open(path)
                            .unwrap()
                    }
                    TangleMode::Append => {
                        let mut path = out_dir.clone();
                        path.push(from_utf8(filename).unwrap());
                        OpenOptions::new()
                            .write(true)
                            .append(true)
                            .open(path)
                            .unwrap()
                    }
                    TangleMode::Prepend => {
                        panic!("prepend mode is unimplemented");
                    }
                    TangleMode::Insert(_) => {
                        panic!("insert mode is unimplemented");
                    }
                };
                if let Some(prefix) = block.properties.prefix {
                    file.write_all(prefix)
                        .context("failed to write prefix for code block to file")?;
                }
                file.write_all(block.part.contents)
                    .context("failed to write code block to file")?;
                if let Some(postfix) = block.properties.postfix {
                    file.write_all(postfix)
                        .context("failed to write postfix for code block to file")?;
                }
                // If execute was set, and the IDs provided match this block's ID, then execute this block's cmd
                match execute(block, &exec_ids)? {
                    Some(output) => print!("{}", output),
                    None => (),
                }
            } else {
                if !cli.no_strict {
                    return Err(anyhow!(
                        "code block without filename found, strict mode enforced"
                    ));
                }
                continue;
            }
        } else {
            if !cli.no_strict {
                return Err(anyhow!(
                    "code block without mode found, strict mode enforced"
                ));
            }
            continue;
        };
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();

    match tangle(cli) {
        Ok(()) => println!("Done"),
        Err(err) => {
            println!("Error: {:#}", err);
            process::exit(1);
        }
    }
}
