mod book;
mod client;
mod epub;
mod utils;
use anyhow::{anyhow, Result};
use clap::Parser;
use indicatif::ParallelProgressIterator;
use log::{debug, error, info, warn};
use rayon::prelude::*;
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use walkdir::WalkDir;

use book::{get_book, get_book_info};
use epub::get_epub;
use utils::dec;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Show debug information
    #[clap(short, long)]
    debug: bool,
    /// Specify (part of) book name, or book ID
    #[clap(short, long)]
    name: Option<String>,
    /// Generate EPUB file
    #[clap(short, long)]
    epub: bool,
}

//To hide the keywords
const DB_DIR: &str = concat!("./databases/novelC", "iwei");
const CPT_DIR: &str = concat!("./files/novelC", "iwei/reader/booksnew/");
const KEY_DIR: &str = concat!("./files/Y2", "hlcy8/");

fn setup_logger(debug: bool) {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}][{}] {}",
                record.target(),
                record.level(),
                message
            ))
        })
        .level(if debug {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .chain(std::io::stderr())
        .apply()
        .unwrap()
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_logger(cli.debug);

    info!("Getting keys of chapters...");
    let keyfiles: Vec<_> = WalkDir::new(KEY_DIR)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();
    let keys: HashMap<_, _> = keyfiles
        .into_par_iter()
        .progress()
        .filter_map(|e| {
            let name = e.file_name().to_str()?;
            let mut b64 = base64::decode(name).ok()?;
            if b64.len() < 9 {
                return None;
            }
            b64.truncate(9);
            let key: u64 = String::from_utf8(b64).ok()?.parse().ok()?;
            let value = std::fs::read_to_string(e.path()).ok()?;
            Some((key, value))
        })
        .collect();

    info!("Connecting to database to get book info...");
    let conn = Connection::open_with_flags(DB_DIR, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    let books_iter = WalkDir::new(CPT_DIR)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().to_str()?.parse::<u64>().ok())
        .map(|book| match get_book_info(book, &conn) {
            Ok((name, author, cover)) => (book, Some((name, author, cover))),
            Err(e) => {
                debug!("Find name and author of book {} fail:{}", book, e);
                (book, None)
            }
        });

    let books: Vec<_> = if let Some(name) = cli.name {
        books_iter
            .filter(|(id, meta)| {
                if name == id.to_string() {
                    return true;
                }
                if let Some((name1, _, _)) = meta.as_ref() {
                    if name1.contains(name.as_str()) {
                        return true;
                    }
                }
                false
            })
            .collect()
    } else {
        books_iter.collect()
    };

    info!("Decrypting all chapters...");
    let cptfiles: Vec<_> = WalkDir::new(CPT_DIR)
        .min_depth(2)
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();
    let cpts: HashMap<_, _> = cptfiles
        .into_par_iter()
        .progress()
        .filter_map(|e| {
            let id: u64 = e.path().file_stem()?.to_str()?.parse().ok()?;
            let key = match keys.get(&id) {
                Some(key) => key,
                None => {
                    debug!("Cannot find key of chapter {}", id);
                    return None;
                }
            };
            let content = std::fs::read(e.path()).ok()?;
            match dec(content, key) {
                Some(c) => Some((id, c)),
                None => {
                    warn!("Decrypt chapter {} fail", id);
                    None
                }
            }
        })
        .collect();

    info!("Exporting txts...");
    books.par_iter().for_each(|(book, meta)| {
        let conn = Connection::open_with_flags(DB_DIR, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let out_name = match meta {
            Some((name, author, _)) => format!("《{}》作者：{}.txt", name, author),
            None => format!("{}.txt", book),
        };
        match get_book(*book, &conn, &cpts) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&out_name, content) {
                    error!("Write book {} error: {}", book, e);
                } else {
                    info!("Export book {}({}) done.", &out_name, book);
                }
            }
            Err(e) => {
                error!("Export book {} error: {}", book, e);
            }
        }
    });

    if cli.epub {
        books.iter().for_each(|(book, meta)| {
            let out_name = match meta {
                Some((name, _, _)) => format!("{}.epub", name),
                None => format!("{}.epub", book),
            };
            info!("Generating {} and getting images...", &out_name);
            match get_epub(*book, &conn, &cpts, meta).and_then(|mut builder| {
                let mut v = Vec::new();
                builder
                    .generate(&mut v)
                    .map_err(|e| anyhow!("EPUB to stream error: {}", e))?;
                std::fs::write(&out_name, v)?;
                Ok(())
            }) {
                Ok(_) => info!("Export epub {}({}) done.", *book, &out_name),
                Err(e) => error!("Export epub {}({}) error: {}", *book, &out_name, e),
            }
        });
    }
    Ok(())
}
