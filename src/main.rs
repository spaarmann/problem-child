mod commands;
mod model;
mod storage;

use env_logger::Env;
use log::{error, info};
use serenity::{client::Client, model::gateway::GatewayIntents};
use std::env;
use std::process;

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("warn")).init();

    info!("Starting up...");

    let token = get_token();

    let pc_data = storage::load_data().unwrap_or_else(|err| {
        error!("Error loading config/pc_data.json file: {:?}", err);
        process::exit(1)
    });

    info!("Loaded subscription information!");

    let mut client = Client::builder(
        &token,
        GatewayIntents::GUILDS
            | GatewayIntents::GUILD_MEMBERS
            | GatewayIntents::GUILD_VOICE_STATES
            | GatewayIntents::GUILD_PRESENCES
            | GatewayIntents::GUILD_MESSAGES
            | GatewayIntents::DIRECT_MESSAGES
            | GatewayIntents::MESSAGE_CONTENT,
    )
    .event_handler(commands::Handler)
    .await
    .expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<commands::DataKey>(pc_data);
    }

    if let Err(err) = client.start().await {
        error!("An error occured while running the client: {:?}", err);
    }
}

fn get_token() -> String {
    if let Ok(path) = env::var("PROBLEM_CHILD_TOKEN_FILE") {
        std::fs::read_to_string(path).unwrap_or_else(|err| {
            error!(
                "PROBLEM_CHILD_TOKEN_FILE specified, but failed to read file: {:?}",
                err
            );
            process::exit(1);
        })
    } else if let Ok(token) = env::var("PROBLEM_CHILD_TOKEN") {
        token
    } else {
        error!("Couldn't get a token from PROBLEM_CHILD_TOKEN or PROBLEM_CHILD_TOKEN_FILE");
        process::exit(1);
    }
}
