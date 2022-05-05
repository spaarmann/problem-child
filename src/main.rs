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

    let pc_data = storage::load_data().unwrap_or_else(|err| {
        error!("Error loading config/pc_data.json file: {:?}", err);
        process::exit(1)
    });

    info!("Loaded subscription information!");

    let mut client = Client::builder(
        &env::var("PROBLEM_CHILD_TOKEN").expect("PROBLEM_CHILD_TOKEN"),
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
