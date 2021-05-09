#[macro_use]
extern crate lazy_static;
extern crate tokio;

use std::io::{stdin, stdout, Write};
use std::process::exit;

use anyhow::{anyhow, Context, Result};
use fake_useragent::UserAgents;
use reqwest::{Client, redirect::Policy, Response};
use scraper::{Html, Selector};

const BASE_URL: &str = "https://episode.cc";

lazy_static! {
    static ref CLIENT: Client = {
       let user_agents = UserAgents::new();
        Client::builder()
        .user_agent(user_agents.random())
        .redirect(Policy::none())
        .build().unwrap()
    };
}

#[tokio::main]
async fn main() -> Result<()> {
    let author_id = ask_author_id()?;
    let about_page = get_about_page(&author_id).await?;
    let nickname = parse_nickname(&about_page)?;

    println!("Let's see what stories does {}({}) have ...", nickname, author_id);
    let stories = parse_stories(&about_page)?;
    let selected_story = ask_select_story(&stories)?;
    parse_story(selected_story).await?;

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
    Ok(CLIENT.get(endpoint).send()
        .await.with_context(|| format!("Failed to send request to {}", endpoint))?)
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
    title_node.text().next()
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
    url: String,
}

fn parse_stories(about_page: &Html) -> Result<Vec<StoryInfo>> {
    let story_selector = Selector::parse("div.stystory").unwrap();
    Ok(about_page.select(&story_selector).map(|e|
        {
            let title_and_url_node = e.first_child().unwrap();
            let relative_url = title_and_url_node.value().as_element().unwrap().attr("href").unwrap();
            let title = e.text().next().unwrap().to_string();
            let url = format!("{}{}", BASE_URL, relative_url);
            StoryInfo {
                title,
                url,
            }
        }).collect())
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

struct StoryPage {
    page: usize,
    content: String,
    background: String,
}

struct Story(Vec<StoryPage>);

async fn parse_story(story: &StoryInfo) -> Result<Story> {
    let first_page_doc = get_page_document(story, 0).await?;
    let story_id = get_story_id(&first_page_doc)?;
    // TODO: get page count
    let page_zero_story = get_page(&story.url, &story_id, 0).await?;
    Err(anyhow!("No story parsed yet"))
}

async fn get_page_document(story: &StoryInfo, page: usize) -> Result<Html> {
    let endpoint = format!("{}/{}", story.url, page);
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
    let id = doc.select(&selector).find(|node| {
        node.value().attr("src").map_or(false, |img_src| img_src != "")
    }).map(|node| {
        let img_src = node.value().attr("src").unwrap();
        // img_src should be "/content/coverimage/{id}.{extension}?{magic_number}"
        let img_name = img_src.split("/").skip(3).next().unwrap();
        let (id, _) = img_name.split_once(".").unwrap();
        id.to_uppercase()
    }).unwrap();
    Ok(id)
}