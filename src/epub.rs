use crate::client::httpget;
use anyhow::{anyhow, Result};
use epub_builder::{EpubBuilder, EpubContent, ReferenceType, ZipLibrary};
use hyper::{
    body::{Buf, Bytes},
    Uri,
};
use log::warn;
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashMap;

enum Section {
    Div(String),
    Cpt { title: String, content: String },
}

struct Sec {
    filename: String,
    inner: Section,
}

fn extract_section(book: u64, conn: &Connection, cpts: &HashMap<u64, String>) -> Result<Vec<Sec>> {
    //Copied from book.rs. Modify the code below carefully, or consider split them into function.
    let mut stmt = conn.prepare(
        "SELECT division_index,division_name FROM division WHERE book_id=? ORDER BY division_index",
    )?;
    let mut rows = stmt.query([book])?;
    let mut divs = Vec::<(u64, String)>::new();
    while let Some(row) = rows.next()? {
        divs.push((row.get(0)?, row.get(1)?));
    }
    //modified: add chapter_index
    let mut stmt = conn.prepare("SELECT chapter_id,chapter_title,chapter_index FROM catalog1 WHERE book_id=? AND division_index=? ORDER BY chapter_index")?;
    //----------

    let mut ret = Vec::new();
    for (div, div_name) in divs {
        ret.push(Sec {
            filename: format!("{}_front.xhtml", div),
            inner: Section::Div(div_name),
        });
        let mut rows = stmt.query([book, div])?;
        while let Some(row) = rows.next()? {
            let (id, title, idx): (String, String, u64) = (row.get(0)?, row.get(1)?, row.get(2)?);
            let id: u64 = id.parse()?;
            ret.push(Sec {
                filename: format!("{}_{}.xhtml", div, idx),
                inner: Section::Cpt {
                    title,
                    content: cpts.get(&id).cloned().unwrap_or_default(),
                },
            });
        }
    }
    Ok(ret)
}

fn build_epub(
    sections: &Vec<Sec>,
) -> Result<(EpubBuilder<ZipLibrary>, Vec<Uri>), epub_builder::Error> {
    let mut builder = EpubBuilder::new(ZipLibrary::new()?)?;
    let mut uris = Vec::new();
    for sec in sections {
        let Sec {
            filename: name,
            inner,
        } = sec;
        let mut ret;
        let content = match inner {
            Section::Div(title) => {
                ret = format!(include_str!("./assets/div.xhtml"), title = title);
                EpubContent::new(name, ret.as_bytes())
                    .title(title)
                    .reftype(ReferenceType::TitlePage)
            }

            // Turn content into HTML. Code below is like a sh*t.
            Section::Cpt { title, content } => {
                ret = String::new();
                for i in content.lines() {
                    static RE_IMG: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*<img").unwrap());
                    let html = if RE_IMG.is_match(i) {
                        static RE_SRC: Lazy<Regex> =
                            Lazy::new(|| Regex::new("src\\s*=\\s*[\"'](.*?)[\"']").unwrap());
                        static RE_ALT: Lazy<Regex> =
                            Lazy::new(|| Regex::new("alt\\s*=\\s*[\"'](.*?)[\"']").unwrap());
                        let src = RE_SRC
                            .captures(i)
                            .and_then(|x| x.get(1)?.as_str().parse::<Uri>().ok())
                            .map(|x| {
                                let path = format!(
                                    "<img src=\"{}\"/>",
                                    x.path().trim_start_matches(&['/', '\\'])
                                );
                                uris.push(x);
                                path
                            });
                        let alt = RE_ALT
                            .captures(i)
                            .and_then(|x| x.get(1))
                            .map(|x| x.as_str());
                        match src {
                            Some(img) => {
                                let alt = alt
                                    .map(|x| format!("<figcaption>{}</figcaption>\n", x))
                                    .unwrap_or_default();
                                format!("<figure>\n{}\n{}</figure>\n", img, alt)
                            }
                            None => alt
                                .map(|x| format!("<p>图：{}</p>\n", x))
                                .unwrap_or_default(),
                        }
                    } else {
                        format!("<p>{}</p>\n", i)
                    };
                    ret.push_str(&html);
                }
                ret = format!(
                    include_str!("./assets/cpt.xhtml"),
                    title = title,
                    content = ret
                );
                #[cfg(test)]
                dbg!(&ret);

                EpubContent::new(name, ret.as_bytes())
                    .title(title)
                    .reftype(ReferenceType::Text)
            }
        };
        builder.add_content(content)?;
    }
    Ok((builder, uris))
}

fn mime_type(path: &str) -> &'static str {
    mime_guess::from_path(path).first_raw().unwrap_or("*/*")
}

#[tokio::main]
async fn get_imgs(uris: Vec<Uri>) -> HashMap<String, Bytes> {
    //Let buffer be 2*LIMIT
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);
    uris.into_iter().for_each(|url| {
        let tx = tx.clone();
        tokio::spawn(async move {
            let path = url.path().trim_start_matches(&['/', '\\']).to_string();
            let r = httpget(url).await;
            tx.send((path, r)).await.ok();
        });
    });
    drop(tx);

    let mut ret = HashMap::new();
    while let Some((path, r)) = rx.recv().await {
        match r {
            Ok(b) => {
                ret.insert(path, b);
            }
            Err(e) => {
                warn!("Get Image {} Err: {}", path, e);
            }
        }
    }
    ret
}

pub fn get_epub(
    book: u64,
    conn: &Connection,
    cpts: &HashMap<u64, String>,
    meta: &Option<(String, String, String)>,
) -> Result<EpubBuilder<ZipLibrary>> {
    let sections = extract_section(book, conn, cpts)?;
    let err_wrapper = |e| anyhow!("Build EPUB Error: {}", e);
    let (mut builder, mut uris) = build_epub(&sections).map_err(err_wrapper)?;
    let cover = if let Some((name, author, url)) = meta {
        builder
            .metadata("author", author.as_str())
            .map_err(err_wrapper)?;
        builder
            .metadata("title", name.as_str())
            .map_err(err_wrapper)?;
        url.parse::<Uri>().ok().map(|uri| {
            let path = uri.path().trim_start_matches(&['/', '\\']).to_string();
            uris.push(uri);
            path
        })
    } else {
        None
    };
    let mut imgs: HashMap<String, Bytes> = get_imgs(uris);
    if let Some(path) = cover {
        if let Some(b) = imgs.remove(&path) {
            builder
                .add_cover_image(&path, b.reader(), mime_type(&path))
                .map_err(err_wrapper)?;
        }
    }
    for (path, content) in imgs {
        builder
            .add_resource(&path, content.reader(), mime_type(&path))
            .map_err(err_wrapper)?;
    }
    Ok(builder)
}
