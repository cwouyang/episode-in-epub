use std::env::current_dir;
use std::fs::File;

use anyhow::Result;
use epub_builder::{EpubBuilder, EpubContent, ZipLibrary};
use epub_builder::EpubVersion::V30;
use scraper::Html;

use crate::parser::GetPageResponse;

pub fn create_epub_file(
    author: &str,
    title: &str,
    cover_image: Option<Vec<u8>>,
    story_pages: Vec<GetPageResponse>,
) -> Result<()> {
    let mut builder = EpubBuilder::new(ZipLibrary::new().unwrap()).unwrap();
    builder
        .epub_version(V30)
        .metadata("author", author)
        .unwrap()
        .metadata("title", title)
        .unwrap()
        .metadata("lang", "zh-TW")
        .unwrap()
        .metadata("generator", "episode-in-epub")
        .unwrap()
        .inline_toc();
    if let Some(cover_image_file) = cover_image {
        builder
            .add_cover_image("data/cover_image.jpg", cover_image_file.as_slice(), "image/jpeg")
            .unwrap();
    }
    for (page, story_page) in story_pages.into_iter().enumerate() {
        let xhtml_file_name = format!("{}.xhtml", page);
        let title = story_page.title;
        let xhtml = surround_with_xhtml_header(&title, &story_page.htmlbody);
        let mut content = EpubContent::new(xhtml_file_name, xhtml.as_bytes());
        content = content.title(title);
        let _ = builder.add_content(content);
    }

    let mut cwd = current_dir()?;
    cwd.push(format!("{}.epub", title));
    let epub_path = File::create(cwd)?;
    builder.generate(epub_path).unwrap();
    Ok(())
}

fn surround_with_xhtml_header(title: &str, body_html: &str) -> String {
    format!("{}<title>{}</title>{}\n{}\n{}", r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
  <meta charset = "utf-8" />
  <meta name="generator" content="episode-in-epub" />"#,
            title,
            r#"<link rel="stylesheet" type="text/css" href="stylesheet.css" />
</head>
<body>"#,
            sanitize_to_meet_xhtml(body_html),
            r#"</body>
</html>"#)
}

fn sanitize_to_meet_xhtml(html: &str) -> String {
    // ensure all tag attributes are enclosed with ""
    let fragment = Html::parse_fragment(html);
    let mut result = String::with_capacity(html.len());
    for n in fragment.root_element().descendants().skip(1) {
        let v = n.value();
        if v.as_element().map_or(false, |e| e.name() == "font" || e.name() == "b" || e.name() == "span") {
            continue;
        }
        if v.is_element() {
            let element = v.as_element().unwrap();
            if element.name() == "br" {
                result.push_str("<br />");
            } else {
                result.push_str(&*format!("{:?}", element));
            }
        } else if v.is_text() {
            result.push_str(&*format!("{}", v.as_text().unwrap().text));
        } else {
            unreachable!();
        }
    }
    result
}
