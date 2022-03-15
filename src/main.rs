mod commands;
mod model;
mod storage;

use log::{error, info};
use serenity::{client::bridge::gateway::GatewayIntents, client::Client};
use simplelog::{Config, LevelFilter, SimpleLogger};
use std::env;
use std::process;

#[tokio::main]
async fn main() {
    SimpleLogger::init(LevelFilter::Warn, Config::default()).unwrap();

    info!("Starting up...");

    let pc_data = storage::load_data().unwrap_or_else(|err| {
        error!("Error loading notif_data.json file: {:?}", err);
        process::exit(1)
    });

    info!("Loaded subscription information!");

    let mut client =
        Client::builder(&env::var("PROBLEM_CHILD_TOKEN").expect("PROBLEM_CHILD_TOKEN"))
            .event_handler(commands::Handler)
            .intents(
                GatewayIntents::GUILDS
                    | GatewayIntents::GUILD_MEMBERS
                    | GatewayIntents::GUILD_VOICE_STATES
                    | GatewayIntents::GUILD_PRESENCES
                    | GatewayIntents::GUILD_MESSAGES
                    | GatewayIntents::DIRECT_MESSAGES,
            )
            .await
            .expect("Error creating client");
    //,
    //    commands::Handler,
    //)
    //.expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<commands::DataKey>(pc_data);
    }

    if let Err(err) = client.start().await {
        error!("An error occured while running the client: {:?}", err);
    }
}
