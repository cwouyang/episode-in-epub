#[macro_use]
extern crate lazy_static;
extern crate tokio;

use std::io::{stdin, stdout, Write};
use std::process::exit;

use anyhow::Result;

use crate::epub::create_epub_file;
use crate::parser::{download_cover_image, get_about_page, parse_author_name, parse_story_infos, parse_story, StoryInfo};

mod parser;
mod epub;

#[tokio::main]
async fn main() -> Result<()> {
    let author_id = ask_author_id()?;
    let about_page = get_about_page(&author_id).await?;
    let author_name = parse_author_name(&about_page)?;

    println!(
        "Let's see what stories does {}({}) have ...",
        author_name, author_id
    );
    let story_infos = parse_story_infos(&about_page).await?;
    let selected_story = ask_select_story(&story_infos)?;

    let story_pages = parse_story(selected_story).await?;
    let cover_image = download_cover_image(&selected_story.id).await;
    create_epub_file(
        &author_name,
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
