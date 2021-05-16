use std::collections::HashMap;
use std::ops::Range;

use anyhow::{anyhow, Context, Result};
use fake_useragent::UserAgents;
use futures::future::join_all;
use reqwest::{Client, redirect::Policy};
use reqwest::header::REFERER;
use reqwest::Response;
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

#[derive(Debug)]
pub struct StoryInfo {
    pub title: String,
    pub id: String,
    pub url: String,
    pub page_range: Range<usize>,
}

pub async fn get_about_page(author_id: &str) -> Result<Html> {
    let endpoint = format!("{}/about/{}", BASE_URL, author_id);
    let resp = send_request(&endpoint).await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("Author \"{}\" did not exists!", author_id));
    }
    let text = resp.text().await.unwrap();
    Ok(Html::parse_document(&text))
}

pub fn parse_author_name(about_page: &Html) -> Result<String> {
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
        .with_context(|| "Failed to parse author name")
}

pub async fn parse_story_infos(about_page: &Html) -> Result<Vec<StoryInfo>> {
    let story_selector = Selector::parse("div.stystory").unwrap();
    let (title_and_urls, get_first_page_futures): (Vec<_>, Vec<_>) = about_page.select(&story_selector).map(|e| {
        let title_and_url_node = e.first_child().unwrap();
        let relative_url = title_and_url_node
            .value()
            .as_element()
            .unwrap()
            .attr("href")
            .unwrap();
        let title = e.text().next().unwrap().to_string();
        let url = format!("{}{}", BASE_URL, relative_url);
        let first_page_doc = get_page_document(url.clone(), 0);
        ((title, url), first_page_doc)
    })
        .unzip();
    join_all(get_first_page_futures).await.into_iter()
        .zip(title_and_urls.into_iter())
        .map(|(first_page_doc, (title, url))| {
            let first_page_doc = first_page_doc?;
            let id = get_story_id(&first_page_doc)?;
            let page_range = get_page_range(&first_page_doc)?;
            Ok(StoryInfo { title, id, url, page_range })
        }).collect::<Result<Vec<StoryInfo>>>()
}

async fn get_page_document(story_url: String, page: usize) -> Result<Html> {
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
pub struct GetPageResponse {
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
    pub htmlbody: String,
    #[serde(rename = "TITLE")]
    pub title: String,
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

pub async fn parse_story(story: &StoryInfo) -> Result<Vec<GetPageResponse>> {
    let mut page_responses = Vec::new();
    for page in story.page_range.start..story.page_range.end {
        match post_get_page_api(&story.url, &story.id, page).await {
            Ok(page_response) => {
                page_responses.push(page_response);
            }
            Err(_) => {
                println!("Failed to download page {}. Skip following pages and continue", page);
                break;
            }
        }
    }
    Ok(page_responses)
}

async fn post_get_page_api(referer: &str, story_id: &str, page: usize) -> Result<GetPageResponse> {
    let page_string = page.to_string();
    let mut form = HashMap::new();
    form.insert("SID", story_id);
    form.insert("PID", &page_string);
    form.insert("StoryPW", "");
    form.insert("PagePW", "");
    form.insert("CountHit", "true");
    Ok(post_request::<GetPageResponse>(GET_PAGE_ENDPOINT, &form, referer).await?)
}

pub async fn download_cover_image(story_id: &str) -> Option<Vec<u8>> {
    let image_url = format!("{}/content/coverimage/{}.jpg", BASE_URL, story_id);
    let response = reqwest::get(image_url).await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.bytes().await.map(|b| b.to_vec()).ok()
}
