use anyhow::{anyhow, Result};
use log::debug;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;

pub fn get_book(book: u64, conn: &Connection, cpts: &HashMap<u64, String>) -> Result<String> {
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
                    debug!("Chapter {} is not found or invalid", id);
                }
            }
        }
    }
    Ok(ret)
}

/// Book info in db. Returns (Name, Author, Cover URL).
pub fn get_book_info(book: u64, conn: &Connection) -> Result<(String, String, String)> {
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
