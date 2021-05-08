#[macro_use]
extern crate lazy_static;
extern crate tokio;

use std::io::{stdin, stdout, Write};

use anyhow::{anyhow, Context, Result};
use fake_useragent::UserAgents;
use reqwest::{Client, redirect::Policy, Response};

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
    check_author_exist(&author_id).await?;
    println!("Let's see what works do \"{}\" have...", { author_id });

    Ok(())
}

fn ask_author_id() -> Result<String> {
    print!("Which author(ID) interests you? ");
    Write::flush(&mut stdout())?;

    let mut author_id = String::new();
    stdin().read_line(&mut author_id)?;
    Ok(author_id.trim().to_string())
}

async fn check_author_exist(author_id: &str) -> Result<()> {
    let endpoint = format!("{}/about/{}", BASE_URL, author_id);
    let resp = send_request(&endpoint).await?;
    let status = resp.status();
    if !status.is_success() {
        return Err(anyhow!("Author \"{}\" did not exists!", author_id));
    }
    let text = resp.text().await.unwrap();
    println!("{:?}", text);
    Ok(())
}

async fn send_request(endpoint: &str) -> Result<Response> {
    Ok(CLIENT.get(endpoint).send()
        .await.with_context(|| format!("Failed to send request to {}", endpoint))?)
}