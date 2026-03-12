use std::collections::{HashMap, HashSet};
use std::env;
use std::fmt::Display;
use std::fs::{self, File};
use std::io::BufReader;
use std::io::Read;
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
enum Mode {
    // Write code blocks out to individual, specified files
    Tangle,
    // Explain the structure of the Markdown file, as significant to Betwixt. Primarily useful for troubleshooting
    Describe,
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Mode::Tangle => "tangle",
                Mode::Describe => "describe",
            }
        )
    }
}

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
    /// The markdown file to parse as input
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
    #[arg(short = 'e', value_delimiter = ',')]
    /// A list of block IDs that should be executed in addition to being tangled
    execute: Option<Vec<String>>,
    /// The mode of operation of betwixt
    #[arg(short = 'm', default_value_t = Mode::Tangle)]
    mode: Mode,
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

fn substitute_anchors(buffer: &mut Vec<u8>, anchor_updates: &HashMap<String, Vec<u8>>) {
    // We iterate backwards to avoid index shifts affecting subsequent replacements
    // Simple but potentially slow approach: search for anchors repeatedly.
    // For now, let's keep it simple.
    for (anchor_name, content) in anchor_updates {
        let start_tag = format!("{} anchor=\"{}\" {}", BETWIXT_TOKEN, anchor_name, CLOSE_TOKEN);
        let start_tag = start_tag.as_bytes();
        let end_tag = CLOSE_TOKEN.as_bytes();

        let mut pos = 0;
        while let Some(start_idx) = buffer[pos..]
            .windows(start_tag.len())
            .position(|w| w == start_tag)
        {
            let start_idx = pos + start_idx;
            let after_start = start_idx + start_tag.len();
            if let Some(end_idx) = buffer[after_start..]
                .windows(end_tag.len())
                .position(|w| w == end_tag)
            {
                let end_idx = after_start + end_idx;
                buffer.splice(after_start..end_idx, content.clone());
                pos = after_start + content.len() + end_tag.len();
            } else {
                break;
            }
            if pos >= buffer.len() {
                break;
            }
        }
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
    let file = File::open(&cli.file).context("unable to open input file")?;
    // Don't change directory yet, wait until we're processing files
    
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
    
    std::env::set_current_dir(&out_dir).context("unable to change to output directory")?;
    
    match cli.mode {
        Mode::Describe => {
            let output = markdown
                .describe(&markdown.root)
                .context("failed building describe output")?;
            println!("{}", output);
        }
        Mode::Tangle => {
            let mut file_blocks: HashMap<String, Vec<&Code>> = HashMap::new();
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
                if let Some(filename) = block.properties.filename {
                    let filename = from_utf8(filename).unwrap().to_owned();
                    file_blocks.entry(filename).or_default().push(block);
                } else if !cli.no_strict {
                    return Err(anyhow!(
                        "code block without filename found, strict mode enforced"
                    ));
                }
            }

            for (filename, blocks) in file_blocks {
                let mut path = out_dir.clone();
                path.push(&filename);
                
                let mut buffer = Vec::new();
                let mut anchor_updates: HashMap<String, Vec<u8>> = HashMap::new();

                // Determine initial state from the first block
                let mut first = true;
                for block in blocks {
                    let mode = block.properties.mode.as_ref().unwrap_or(&TangleMode::Append);
                    let anchor = block.properties.anchor.map(|a| from_utf8(a).unwrap().to_owned());

                    if first {
                        if matches!(mode, TangleMode::Overwrite) && anchor.is_none() {
                            // Start with empty buffer
                        } else if path.exists() {
                            buffer = fs::read(&path).context(format!("failed to read existing file {}", filename))?;
                        }
                        first = false;
                    }

                    let mut content = Vec::new();
                    if let Some(prefix) = block.properties.prefix {
                        content.extend_from_slice(prefix);
                    }
                    content.extend_from_slice(block.part.contents);
                    if let Some(postfix) = block.properties.postfix {
                        content.extend_from_slice(postfix);
                    }

                    match anchor {
                        None => {
                            match mode {
                                TangleMode::Overwrite => {
                                    buffer = content;
                                    anchor_updates.clear();
                                }
                                TangleMode::Append => {
                                    buffer.extend_from_slice(&content);
                                }
                                TangleMode::Prepend => {
                                    let mut new_buffer = content;
                                    new_buffer.extend_from_slice(&buffer);
                                    buffer = new_buffer;
                                }
                                TangleMode::Insert(_) => panic!("legacy insert mode not supported in new flow"),
                            }
                        }
                        Some(name) => {
                            match mode {
                                TangleMode::Overwrite => {
                                    anchor_updates.insert(name, content);
                                }
                                TangleMode::Append => {
                                    anchor_updates.entry(name).or_default().extend_from_slice(&content);
                                }
                                _ => return Err(anyhow!("only overwrite and append modes are supported for anchors")),
                            }
                        }
                    }

                    // Execute if requested
                    // Before executing, we must ensure the file is written so it can be used as a dependency
                    let mut exec_buffer = buffer.clone();
                    substitute_anchors(&mut exec_buffer, &anchor_updates);
                    fs::write(&path, exec_buffer).context(format!("failed to write intermediate file {}", filename))?;

                    match execute(block, &exec_ids)? {
                        Some(output) => print!("{}", output),
                        None => (),
                    }
                }

                substitute_anchors(&mut buffer, &anchor_updates);
                fs::write(&path, buffer).context(format!("failed to write to file {}", filename))?;
            }
        }
    };

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
