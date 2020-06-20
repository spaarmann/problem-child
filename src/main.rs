mod commands;
mod model;
mod storage;

use log::{error, info};
use serenity::client::Client;
use simple_logger;
use std::env;
use std::process;

fn main() {
    simple_logger::init().unwrap();

    info!("Starting up...");

    let notif_data = storage::load_notif_data().unwrap_or_else(|err| {
        error!("Error loading notif_data.json file: {:?}", err);
        process::exit(1)
    });

    info!(
        "Loaded subscription information for {} channels.",
        notif_data.len()
    );

    let mut client = Client::new(
        &env::var("PROBLEM_CHILD_TOKEN").expect("PROBLEM_CHILD_TOKEN"),
        commands::Handler,
    )
    .expect("Error creating client");

    {
        let mut data = client.data.write();
        data.insert::<commands::NotifData>(notif_data);
    }

    if let Err(err) = client.start() {
        error!("An error occured while running the client: {:?}", err);
    }
}
