mod commands;
mod model;
mod storage;

use std::env;
use std::process;

use serenity::client::Client;

fn main() {
    println!("Starting up...");

    let notif_data = storage::load_notif_data().unwrap_or_else(|err| {
        println!("Error loading notif_data.json file: {:?}", err);
        process::exit(1)
    });

    println!(
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
        println!("An error occured while running the client: {:?}", err);
    }
}
