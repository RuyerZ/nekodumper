use crate::client::httpget;
use anyhow::{anyhow, Result};
use epub_builder::{EpubBuilder, EpubContent, ReferenceType, ZipLibrary};
use hyper::{body::Bytes, Uri};
use once_cell::sync::Lazy;
use log::warn;
use regex::Regex;
use rusqlite::Connection;
use std::collections::HashMap;

enum Section {
    Div(String),
    Cpt { title: String, content: String },
}

fn extract_section(
    book: u64,
    conn: &Connection,
    cpts: &HashMap<u64, String>,
) -> Result<Vec<Section>> {
    //Copied from book.rs. Modify the code below carefully, or consider split them into function.
    let mut stmt = conn.prepare(
        "SELECT division_index,division_name FROM division WHERE book_id=? ORDER BY division_index",
    )?;
    let mut rows = stmt.query([book])?;
    let mut divs = Vec::<(u64, String)>::new();
    while let Some(row) = rows.next()? {
        divs.push((row.get(0)?, row.get(1)?));
    }
    let mut stmt = conn.prepare("SELECT chapter_id,chapter_title FROM catalog1 WHERE book_id=? AND division_index=? ORDER BY chapter_index")?;
    //----------

    let mut ret = Vec::new();
    for (div, div_name) in divs {
        ret.push(Section::Div(div_name));
        let mut rows = stmt.query([book, div])?;
        while let Some(row) = rows.next()? {
            let (id, title): (String, String) = (row.get(0)?, row.get(1)?);
            let id: u64 = id.parse()?;
            ret.push(Section::Cpt {
                title,
                content: cpts.get(&id).cloned().unwrap_or_default(),
            });
        }
    }
    Ok(ret)
}

fn build_epub(
    sections: &Vec<Section>,
) -> Result<(EpubBuilder<ZipLibrary>, Vec<Uri>), Box<dyn std::error::Error>> {
    let mut builder = EpubBuilder::new(ZipLibrary::new()?)?;
    let mut uris = Vec::new();
    let mut id: usize = 0;
    for sec in sections {
        id += 1;
        let name = id.to_string() + ".xhtml";
        let mut ret = String::new();
        let content = match sec {
            Section::Div(title) => EpubContent::new(name, title.as_bytes())
                .title(title)
                .reftype(ReferenceType::TitlePage),

            // Turn content into HTML. Code below is like a sh*t.
            Section::Cpt { title, content } => {
                for i in content.lines() {
                    static RE_IMG: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*<img").unwrap());
                    let html = if RE_IMG.is_match(i) {
                        static RE_SRC: Lazy<Regex> =
                            Lazy::new(|| Regex::new("src\\s*=\\s*[\"'](.*?)[\"']").unwrap());
                        static RE_ALT: Lazy<Regex> =
                            Lazy::new(|| Regex::new("alt\\s*=\\s*[\"'](.*?)[\"']").unwrap());
                        let src = RE_SRC
                            .captures(i)
                            .and_then(|x| Some(x.get(1)?.as_str().parse::<Uri>().ok()?))
                            .map(|x| {
                                let path = format!("<img src=\"{}\">", x.path());
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

#[test]
fn test_build_epub() {
    let dummy_div = Section::Div(String::from("Div Title"));
    let dummy_cpt = Section::Cpt {
        title: "Cpt Title".to_string(),
        content: "第一段\n123\n  <img src=\"https://example.com/img/1.jpg\" alt='hello'>\n  ORZ"
            .to_string(),
    };
    let v = vec![dummy_div, dummy_cpt];
    dbg!(build_epub(&v).unwrap());
}

#[tokio::main]
async fn get_imgs(uris: Vec<Uri>) -> HashMap<String, Bytes> {
    //Let buffer be 2*LIMIT
    let (tx,mut rx) = tokio::sync::mpsc::channel(32);
    uris.into_iter().for_each(|url| {
        let tx=tx.clone();
        tokio::spawn(async move{
            let path = url.path().to_string();
            let r = httpget(url).await;
            tx.send((path,r)).await.ok();
        });
    });
    drop(tx);

    let mut ret = HashMap::new();
    while let Some((path,r)) = rx.recv().await {
        match r {
            Ok(b) => {ret.insert(path,b);},
            Err(e) => {warn!("Get Image {} Err: {}",path,e);}
        }
    }
    ret
}

pub fn get_epub(
    book: u64,
    conn: &Connection,
    cpts: &HashMap<u64, String>,
    meta: Option<(String, String, String)>,
) -> Result<EpubBuilder<ZipLibrary>> {
    let sections = extract_section(book, conn, cpts)?;
    let (mut builder,mut uris) =
        build_epub(&sections).map_err(|e| anyhow!("Build EPUB Error in extracting: {:?}", e))?;
    if let Some((_,_,cover)) = &meta {
        if let Ok(uri) = cover.parse::<Uri>() {
            uris.push(uri);
        }
    }
    todo!();
}
