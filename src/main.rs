use std::{
    fs::File,
    io::{prelude::*, BufRead, BufReader, SeekFrom, Write},
    path::{self, Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use regex::Regex;

#[derive(Copy, Clone, PartialEq, Debug, ValueEnum)]
#[clap(rename_all = "snake_case")]
enum Target {
    Auto,
    Stignore,
    StignoreSync,
}

/// Adds syncthing ignore patterns (https://docs.syncthing.net/users/ignoring)
/// to parent syncthing folder of the current working directory.
///
/// Source code & examples: https://github.com/Andrew-Morozko/stignore
#[derive(Parser, Debug)]
#[clap(version, about, global_setting(clap::AppSettings::DeriveDisplayOrder))]
struct Args {
    /// Patterns to add
    #[clap(value_parser, required(true), min_values(1))]
    pattern: Vec<String>,

    /// Specify which file would be appended with patterns
    ///
    /// auto - append patterns to .stignore_sync if it is included in .stignore,
    /// otherwise append to .stignore (create if doesn't exist)
    ///
    /// stignore - append patterns to .stignore, create if doesn't exist
    ///
    /// stignore_sync - append patterns to .stignore_sync, create if doesn't exist
    #[clap(short, long, arg_enum, value_parser, default_value_t = Target::Auto)]
    target: Target,

    /// Copy patterns as-is
    ///
    /// Don't prepend path to CWD relative to syncthing folder root
    #[clap(short, long, value_parser)]
    absolute: bool,

    /// Display planned changes and wait for confirmation
    #[clap(short, long, value_parser, conflicts_with("silent"))]
    preview: bool,

    /// Don't display messages
    #[clap(short, long, value_parser)]
    silent: bool,
}

#[cfg(windows)]
const LINE_ENDING: &str = "\r\n";
#[cfg(not(windows))]
const LINE_ENDING: &str = "\n";

fn find_syncthing_dir() -> Result<(PathBuf, PathBuf)> {
    let cwd = std::env::current_dir()
        .and_then(std::fs::canonicalize)
        .context("Can't determine current working directory")?;
    let mut st_dir = cwd.clone();
    loop {
        st_dir.push(".stfolder");
        let found = st_dir.is_dir();
        st_dir.pop();
        if found {
            break;
        }
        if !st_dir.pop() {
            bail!("Current directory is not inside of a syncthing folder");
        }
    }

    let prefix = path::Path::join(
        path::Path::new(path::Component::RootDir.as_os_str()),
        cwd.strip_prefix(&st_dir).unwrap(),
    );

    Ok((st_dir, prefix))
}

fn process_patterns(patterns: &[String], prepend_prefix: Option<&PathBuf>) -> Result<String> {
    let re = Regex::new(r"^((?:#include )|(?:(?:\(\?[di]\)|!))*) *(.+)$").unwrap();

    let mut out_str = String::new();
    let mut errs = Vec::new();

    let patterns = patterns.iter().flat_map(|t| t.split('\n'));

    for mut pattern in patterns {
        pattern = pattern.trim();
        if pattern.is_empty() || pattern.starts_with("//") {
            // empty pattern results in an extra new line inserted
            out_str.push_str(pattern);
            out_str.push_str(LINE_ENDING);
            continue;
        }
        let m = re.captures(pattern);
        if m.is_none() {
            errs.push(pattern);
            continue;
        }
        let m = m.unwrap();
        let pattern_path = m.get(2);
        if pattern_path.is_none() {
            errs.push(pattern);
            continue;
        }
        let pattern_path = pattern_path.unwrap();

        if let Some(m) = m.get(1) {
            out_str.push_str(m.as_str());
        }

        match prepend_prefix {
            None => {
                out_str.push_str(pattern_path.as_str());
            }
            Some(prefix) => {
                let pattern_path = Path::new(pattern_path.as_str());
                out_str.push_str(
                    prefix
                        .components()
                        .chain(pattern_path.components().skip_while(|m| {
                            matches!(
                                m,
                                path::Component::RootDir
                                    | path::Component::Prefix(_)
                                    | path::Component::CurDir
                            )
                        }))
                        .collect::<PathBuf>()
                        .display()
                        .to_string()
                        .as_str(),
                );
            }
        }
        out_str.push_str(LINE_ENDING);
    }

    if !errs.is_empty() {
        bail!(
            "Incorrect pattern{}:\n{}",
            if errs.len() > 1 { "s" } else { "" },
            errs.join("\n")
        );
    }
    if out_str.trim().is_empty() {
        bail!("No patterns supplied!")
    }
    Ok(out_str)
}

enum PathOrFile {
    Path(PathBuf),
    File(PathBuf, File),
}

impl PathOrFile {
    fn open(&mut self) -> Result<&mut File, std::io::Error> {
        match self {
            Self::File(_, ref mut f) => Ok(f),
            Self::Path(ref mut p) => {
                let f = File::options()
                    .read(true)
                    .write(true)
                    .create(true)
                    .open(&p)?;
                *self = Self::File(std::mem::take(p), f);
                if let Self::File(_, f) = self {
                    return Ok(f);
                }
                unreachable!()
            }
        }
    }
    fn path(&self) -> &Path {
        match self {
            Self::File(ref p, _) => p,
            Self::Path(ref p) => p,
        }
        .as_path()
    }
}

fn is_stignore_sync_included(stignore: &mut PathOrFile) -> Result<bool> {
    let re = Regex::new(r"^\s*#include\s+\.stignore_sync\s*$").unwrap();
    let f = stignore.open()?;

    Ok(BufReader::new(f)
        .lines()
        .find_map(|p| match p {
            Ok(ref t) => {
                if re.is_match(t) {
                    Some(Ok(()))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),
        })
        .transpose()?
        .is_some())
}

fn append(f: &mut PathOrFile, patterns: &String) -> Result<()> {
    let f = f.open()?;
    let file_len = f.seek(SeekFrom::End(0))?;
    let prepend_new_line = if file_len == 0 {
        false
    } else if file_len < (LINE_ENDING.len() as u64) {
        true
    } else {
        let mut buf = [0u8; LINE_ENDING.len()];
        f.seek(SeekFrom::End(-(LINE_ENDING.len() as i64)))?;
        f.read_exact(&mut buf)?;
        !buf.ends_with(LINE_ENDING.as_bytes())
    };

    if prepend_new_line {
        f.write_all(LINE_ENDING.as_bytes())?;
    };

    f.write_all(patterns.as_bytes())?;

    Ok(())
}

fn go(args: &Args) -> Result<()> {
    let (st_dir, prefix) = find_syncthing_dir()?;

    let patterns = process_patterns(
        &args.pattern,
        if args.absolute { None } else { Some(&prefix) },
    )?;

    let mut stignore = PathOrFile::Path(st_dir.join(".stignore"));
    let stignore_sync = st_dir.join(".stignore_sync");

    let resolved_target = if args.target == Target::Auto {
        let sync_included =
            is_stignore_sync_included(&mut stignore).context("Can't read .stignore file")?;
        if sync_included {
            Target::StignoreSync
        } else {
            if !args.silent && stignore_sync.is_file() {
                eprintln!(
                    "NOTE: .stignore_sync exists, but wasn't included in .stignore. \
                    Working with .stignore"
                );
            }
            Target::Stignore
        }
    } else {
        args.target
    };

    let mut tgt_file = match resolved_target {
        Target::Stignore => stignore,
        Target::StignoreSync => {
            drop(stignore);
            PathOrFile::Path(stignore_sync)
        }
        Target::Auto => unreachable!("Target::Auto was resolved into concrete targets"),
    };

    if !args.silent {
        println!("Appending to {}:\n{patterns}", tgt_file.path().display());
    }
    if args.preview {
        use question::{Answer, Question};
        let res = Question::new("Proceed?")
            .until_acceptable()
            .default(Answer::YES)
            .show_defaults()
            .confirm();
        if res == Answer::NO {
            println!("Aborting.");
            return Ok(());
        }
    }
    append(&mut tgt_file, &patterns).context("Can't append to file")
}

fn main() -> Result<()> {
    let args = Args::parse();
    let res = go(&args);
    if args.silent && res.is_err() {
        std::process::exit(1);
    }
    res
}
