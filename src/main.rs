#[macro_use]
extern crate lazy_static;
extern crate tokio;

use std::collections::HashMap;
use std::env::current_dir;
use std::fs::File;
use std::io::{stdin, stdout, Write};
use std::ops::Range;
use std::process::exit;

use anyhow::{anyhow, Context, Result};
use epub_builder::{EpubBuilder, EpubContent, ZipLibrary};
use epub_builder::EpubVersion::V30;
use fake_useragent::UserAgents;
use reqwest::{Client, redirect::Policy, Response};
use reqwest::header::REFERER;
use scraper::{Html, Selector};
use serde::de::DeserializeOwned;
use serde::Deserialize;

const BASE_URL: &str = "https://episode.cc";
const GET_PAGE_ENDPOINT: &str = "https://episode.cc/Reading/GetPage";

lazy_static! {
    static ref CLIENT: Client = {
        let user_agents = UserAgents::new();
        Client::builder()
            .user_agent(user_agents.random())
            .redirect(Policy::none())
            .build()
            .unwrap()
    };
}

#[tokio::main]
async fn main() -> Result<()> {
    let author_id = ask_author_id()?;
    let about_page = get_about_page(&author_id).await?;
    let nickname = parse_nickname(&about_page)?;

    println!(
        "Let's see what stories does {}({}) have ...",
        nickname, author_id
    );
    let stories = parse_stories(&about_page).await?;
    let selected_story = ask_select_story(&stories)?;

    let story_pages = parse_story(selected_story).await?;
    let cover_image = download_cover_image(&selected_story.id).await;
    create_epub_file(
        &nickname,
        &selected_story.title,
        cover_image,
        story_pages,
    )?;
    Ok(())
}

fn ask_author_id() -> Result<String> {
    print!("Which author(ID) interests you? ");
    Write::flush(&mut stdout())?;

    let mut author_id = String::new();
    stdin().read_line(&mut author_id)?;
    Ok(author_id.trim().to_string())
}

async fn send_request(endpoint: &str) -> Result<Response> {
    Ok(CLIENT
        .get(endpoint)
        .send()
        .await
        .with_context(|| format!("Failed to send request to {}", endpoint))?)
}

async fn post_request<T: DeserializeOwned>(
    endpoint: &str,
    form: &HashMap<&str, &str>,
    referer: &str,
) -> Result<T> {
    Ok(CLIENT
        .post(endpoint)
        .form(form)
        .header(REFERER, referer)
        .send()
        .await
        .with_context(|| format!("Failed to post request to {}", endpoint))?
        .json::<T>()
        .await?)
}

async fn get_about_page(author_id: &str) -> Result<Html> {
    let endpoint = format!("{}/about/{}", BASE_URL, author_id);
    let resp = send_request(&endpoint).await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("Author \"{}\" did not exists!", author_id));
    }
    let text = resp.text().await.unwrap();
    Ok(Html::parse_document(&text))
}

fn parse_nickname(about_page: &Html) -> Result<String> {
    let title_selector = Selector::parse("title").unwrap();
    let title_node = about_page.select(&title_selector).next().unwrap();
    title_node
        .text()
        .next()
        .map(|t| {
            // t should equal to "關於 xxx"
            t.split_ascii_whitespace()
        })
        .map(|s| s.skip(1).next().unwrap().to_string())
        .with_context(|| "Failed to parse nickname")
}

#[derive(Debug)]
struct StoryInfo {
    title: String,
    id: String,
    url: String,
    page_range: Range<usize>,
}

async fn parse_stories(about_page: &Html) -> Result<Vec<StoryInfo>> {
    let story_selector = Selector::parse("div.stystory").unwrap();
    let mut story_infos = Vec::new();
    for e in about_page.select(&story_selector) {
        let title_and_url_node = e.first_child().unwrap();
        let relative_url = title_and_url_node
            .value()
            .as_element()
            .unwrap()
            .attr("href")
            .unwrap();
        let title = e.text().next().unwrap().to_string();
        let url = format!("{}{}", BASE_URL, relative_url);
        let first_page_doc = get_page_document(&url, 0).await?;
        let id = get_story_id(&first_page_doc)?;
        let page_range = get_page_range(&first_page_doc)?;
        story_infos.push(StoryInfo { title, id, url, page_range });
    }
    Ok(story_infos)
}

fn ask_select_story(stories: &Vec<StoryInfo>) -> Result<&StoryInfo> {
    println!("Which story do you want to read? Or enter `q` to exit");
    for (i, story) in stories.iter().enumerate() {
        println!("{}) {}", i + 1, story.title);
    }
    loop {
        print!("> ");
        Write::flush(&mut stdout())?;
        let mut story_index_str = String::new();
        stdin().read_line(&mut story_index_str)?;

        let trim_story_index_str = story_index_str.trim();
        match trim_story_index_str.parse::<usize>() {
            Ok(i) if i <= stories.len() => {
                return Ok(&stories[i - 1]);
            }
            _ => {
                if trim_story_index_str == "q" {
                    exit(1);
                }
                println!("Try again");
            }
        };
    }
}

async fn parse_story(story: &StoryInfo) -> Result<Vec<GetPageResponse>> {
    let mut page_responses = Vec::new();
    for page in story.page_range.start..story.page_range.end {
        match get_page(&story.url, &story.id, page).await {
            Ok(page_response) => {
                page_responses.push(page_response);
            }
            Err(_) => {
                println!("Failed to download page {}. Skip following pages.", page);
                break;
            }
        }
    }
    Ok(page_responses)
}

async fn get_page_document(story_url: &str, page: usize) -> Result<Html> {
    let endpoint = format!("{}/{}", story_url, page);
    let resp = send_request(&endpoint).await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("Failed to get page {} content", page));
    }
    let text = resp.text().await.unwrap();
    Ok(Html::parse_document(&text))
}

fn get_story_id(doc: &Html) -> Result<String> {
    let selector = Selector::parse("img.roundcorner").unwrap();
    let id = doc
        .select(&selector)
        .find(|node| {
            node.value()
                .attr("src")
                .map_or(false, |img_src| img_src != "")
        })
        .map(|node| {
            let img_src = node.value().attr("src").unwrap();
            // img_src should be "/content/coverimage/{id}.{extension}?{magic_number}"
            let img_name = img_src.split("/").skip(3).next().unwrap();
            let (id, _) = img_name.split_once(".").unwrap();
            id.to_uppercase()
        })
        .unwrap();
    Ok(id)
}

fn get_page_range(doc: &Html) -> Result<Range<usize>> {
    let selector = Selector::parse("div[style=\"float:left\"]").unwrap();
    for n in doc.select(&selector) {
        //.filter(|n| n.inner_html().contains("頁")) {
        let inner_html = n.inner_html();
        if inner_html.contains("頁") {
            let (page_count_str, _) = inner_html.split_once(" ").unwrap();
            let page_count = page_count_str.parse::<usize>()?;
            return Ok(0..page_count);
        }
    }
    Ok(0..1)
}

#[derive(Deserialize, Debug)]
struct GetPageResponse {
    #[serde(rename = "FC")]
    fc: String,
    #[serde(rename = "FC1")]
    fc1: String,
    #[serde(rename = "FC2")]
    fc2: String,
    #[serde(rename = "BG")]
    bg: String,
    #[serde(rename = "VMOBISHIFT")]
    vmobishift: String,
    #[serde(rename = "PARABGOP")]
    parabgop: String,
    #[serde(rename = "IMAGESOURCE")]
    imagesource: String,
    #[serde(rename = "EMBEDIMGSOURCE")]
    embedimgsource: String,
    #[serde(rename = "PHOTOGRAPHER")]
    photographer: String,
    #[serde(rename = "DMMODEL")]
    dmmodel: String,
    #[serde(rename = "IDENT")]
    ident: i32,
    #[serde(rename = "HEIGHT")]
    height: i32,
    #[serde(rename = "HTMLBODY")]
    htmlbody: String,
    #[serde(rename = "TITLE")]
    title: String,
    #[serde(rename = "VRTPTITLE")]
    vrtptitle: i32,
    #[serde(rename = "PAGELOCK")]
    pagelock: i32,
    #[serde(rename = "PWHINT")]
    pwhint: String,
    #[serde(rename = "KEYINPUT")]
    keyinput: i32,
    #[serde(rename = "PAGEACCESSTYPE")]
    pageaccesstype: i32,
    #[serde(rename = "MyPRAISE")]
    mypraise: i32,
    #[serde(rename = "PRAISECOUNT")]
    praisecount: i32,
    #[serde(rename = "UID")]
    uid: String,
    #[serde(rename = "TODAYHITS")]
    todayhits: String,
    #[serde(rename = "TOTALHITS")]
    totalhits: String,
    #[serde(rename = "RATE")]
    rate: String,
    #[serde(rename = "COMMENTSIZE")]
    commentsize: i32,
    #[serde(rename = "GATHERST")]
    gatherst: i32,
    #[serde(rename = "PLUGINDATA")]
    plugindata: String,
    #[serde(rename = "StoryPWpass")]
    story_pw_pass: bool,
}

async fn get_page(referer: &str, story_id: &str, page: usize) -> Result<GetPageResponse> {
    let page_string = page.to_string();
    let mut form = HashMap::new();
    form.insert("SID", story_id);
    form.insert("PID", &page_string);
    form.insert("StoryPW", "");
    form.insert("PagePW", "");
    form.insert("CountHit", "true");
    Ok(post_request::<GetPageResponse>(GET_PAGE_ENDPOINT, &form, referer).await?)
}

async fn download_cover_image(story_id: &str) -> Option<Vec<u8>> {
    let image_url = format!("{}/content/coverimage/{}.jpg", BASE_URL, story_id);
    let response = reqwest::get(image_url).await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.bytes().await.map(|b| b.to_vec()).ok()
}

fn create_epub_file(
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