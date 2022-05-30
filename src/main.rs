mod utils;
use anyhow::anyhow;
use rayon::prelude::*;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::collections::HashMap;
use utils::decrypt;
use walkdir::WalkDir;

//To hide the keywords
const DB_DIR: &str = concat!("./databases/novelC", "iwei");
const CPT_DIR: &str = concat!("./files/novelC", "iwei/reader/booksnew/");
const KEY_DIR: &str = concat!("./files/Y2", "hlcy8/");

fn get_book(book: u64, conn: &Connection, keys: &HashMap<u64, String>) -> anyhow::Result<String> {
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
            let get_chapter = || -> anyhow::Result<String> {
                let ciphertext =
                    std::fs::read_to_string(format!("{}{}/{}.txt", CPT_DIR, &book, &id))
                        .map_err(|e| anyhow!("Find chapter {} Error:{}", &id, e))?;
                let key = keys
                    .get(&id)
                    .ok_or_else(|| anyhow!("Cannot find key of chapter {}", &id))?;
                decrypt(ciphertext, key).ok_or_else(|| anyhow!("Decrypt chapter {} fail", &id))
            };
            match get_chapter() {
                Ok(content) => {
                    ret.push_str(&content);
                    ret.push_str("\n\n");
                }
                Err(e) => {
                    println!("{}", e);
                }
            }
        }
    }
    Ok(ret)
}

fn get_book_info(book: u64, conn: &Connection) -> anyhow::Result<(String, String)> {
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
    Ok((name.to_string(), author.to_string()))
}

fn main() -> anyhow::Result<()> {
    let keys: HashMap<_, _> = WalkDir::new(KEY_DIR)
        .into_iter()
        .par_bridge()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_str()?;
            let mut b64 = base64::decode(name).ok()?;
            if b64.len() < 9 {
                return None;
            }
            b64.resize(9, 0);
            let key: u64 = String::from_utf8(b64).ok()?.parse().ok()?;
            let value = String::from_utf8(std::fs::read(e.path()).ok()?).ok()?;
            Some((key, value))
        })
        .collect();

    let books: Vec<_> = WalkDir::new(CPT_DIR)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_str()?;
            let id: u64 = name.parse().ok()?;
            Some(id)
        })
        .collect();

    //test connection
    let _ = Connection::open_with_flags(DB_DIR, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    books.into_par_iter().for_each(|book| {
        let conn = Connection::open_with_flags(DB_DIR, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        println!("Extracting book {}:", book);
        match get_book(book, &conn, &keys) {
            Ok(content) => {
                let filename = match get_book_info(book, &conn) {
                    Ok((name, author)) => {
                        format!("《{}》作者：{}.txt", name, author)
                    }
                    Err(_) => {
                        format!("{}.txt", book)
                    }
                };
                if let Err(e) = std::fs::write(filename, content) {
                    println!("Write book {} error:{}", book, e);
                }
            }
            Err(e) => {
                println!("Get book {} error:{}", book, e);
            }
        }
    });
    Ok(())
}
