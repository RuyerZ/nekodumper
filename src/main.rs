mod utils;
use anyhow::{anyhow, Result};
use clap::Parser;
use log::{debug, error, info, warn};
use rayon::prelude::*;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::collections::HashMap;
use utils::decrypt;
use walkdir::WalkDir;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    /// Turn debug informations on
    #[clap(short, long)]
    debug: bool,
    /// Specify (part of) book name, or book ID
    #[clap(short, long)]
    name: Option<String>,
}

//To hide the keywords
const DB_DIR: &str = concat!("./databases/novelC", "iwei");
const CPT_DIR: &str = concat!("./files/novelC", "iwei/reader/booksnew/");
const KEY_DIR: &str = concat!("./files/Y2", "hlcy8/");

fn get_book(book: u64, conn: &Connection, cpts: &HashMap<u64, String>) -> Result<String> {
    let mut stmt = conn.prepare(
        "SELECT division_index,division_name FROM division WHERE book_id=? ORDER BY division_index",
    )?;
    let mut rows = stmt.query([book])?;
    let mut divs = Vec::<(u64, String)>::new();
    while let Some(row) = rows.next()? {
        divs.push((row.get(0)?, row.get(1)?));
    }
    let mut stmt = conn.prepare("SELECT chapter_id,chapter_title FROM catalog1 WHERE book_id=? AND division_index=? ORDER BY chapter_index")?;
    let mut ret = String::new();
    for (div, div_name) in divs {
        ret.push_str(&div_name);
        ret.push_str("\n\n");
        let mut rows = stmt.query([book, div])?;
        while let Some(row) = rows.next()? {
            let (id, title): (String, String) = (row.get(0)?, row.get(1)?);
            let id: u64 = id.parse()?;
            ret.push_str(&title);
            ret.push_str("\n\n");
            match cpts.get(&id) {
                Some(content) => {
                    ret.push_str(content);
                    ret.push_str("\n\n");
                }
                None => {
                    debug!("Chapter {} is invalid", id);
                }
            }
        }
    }
    Ok(ret)
}

/// Book info in db. Returns (Name, Author, Cover URL).
fn get_book_info(book: u64, conn: &Connection) -> Result<(String, String, String)> {
    let mut book_info: Option<String> = conn
        .query_row(
            "SELECT book_info from shelf_book_info where book_id=?",
            [book],
            |row| row.get(0),
        )
        .optional()?;
    if book_info.is_none() {
        book_info = conn
            .query_row(
                "SELECT book_info from read_history where book_id=?",
                [book],
                |row| row.get(0),
            )
            .optional()?;
    }
    let book_info = book_info.ok_or_else(|| anyhow!("Cannot find info about book {}", book))?;
    let json: serde_json::Value = serde_json::from_str(&book_info)?;
    let name = json["book_name"]
        .as_str()
        .ok_or_else(|| anyhow!("Parse json error"))?;
    let author = json["author_name"]
        .as_str()
        .ok_or_else(|| anyhow!("Parse json error"))?;
    let cover = json["cover"]
        .as_str()
        .ok_or_else(|| anyhow!("Parse json error"))?;
    Ok((name.to_string(), author.to_string(), cover.to_string()))
}

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
        .chain(std::io::stdout())
        .apply()
        .unwrap()
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    setup_logger(cli.debug);
    let keys: HashMap<_, _> = WalkDir::new(KEY_DIR)
        .min_depth(1)
        .into_iter()
        .par_bridge()
        .filter_map(|e| e.ok())
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
                    if name.contains(name1.as_str()) {
                        return true;
                    }
                }
                false
            })
            .collect()
    } else {
        books_iter.collect()
    };

    let cpts: HashMap<_, _> = WalkDir::new(CPT_DIR)
        .min_depth(2)
        .into_iter()
        .par_bridge()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let id: u64 = e.path().file_stem()?.to_str()?.parse().ok()?;
            let key = match keys.get(&id) {
                Some(key) => key,
                None => {
                    debug!("Cannot find key of chapter {}", id);
                    return None;
                }
            };
            let content = std::fs::read_to_string(e.path()).ok()?;
            match decrypt(content, key) {
                Some(c) => Some((id, c)),
                None => {
                    warn!("Decrypt chapter {} fail", id);
                    None
                }
            }
        })
        .collect();

    books.into_par_iter().for_each(|(book, meta)| {
        let conn = Connection::open_with_flags(DB_DIR, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let out_name = match &meta {
            Some((name, author, _)) => format!("《{}》作者：{}.txt", name, author),
            None => format!("{}.txt", book),
        };
        match get_book(book, &conn, &cpts) {
            Ok(content) => {
                if let Err(e) = std::fs::write(&out_name, content) {
                    warn!("Write book {} error: {}", book, e);
                } else {
                    info!("Export book {}({}) done.", &out_name, book);
                }
            }
            Err(e) => {
                error!("Export book {} error: {}", book, e);
            }
        }
    });
    Ok(())
}
