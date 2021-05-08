#[macro_use]
extern crate lazy_static;
extern crate tokio;

use std::io::{stdin, stdout, Write};

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

    println!("Let's see what stories does \"{}\" have...", { nickname });
    let stories = parse_stories(&about_page)?;
    for s in stories {
        println!("{:?}", s);
    }

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
struct Story {
    title: String,
    url: String,
}

fn parse_stories(about_page: &Html) -> Result<Vec<Story>> {
    let story_selector = Selector::parse("div.stystory").unwrap();
    Ok(about_page.select(&story_selector).map(|e|
        {
            let title_and_url_node = e.first_child().unwrap();
            let relative_url = title_and_url_node.value().as_element().unwrap().attr("href").unwrap();
            let title = e.text().next().unwrap().to_string();
            let url = format!("{}{}", BASE_URL, relative_url);
            Story {
                title,
                url,
            }
        }).collect())
}