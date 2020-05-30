use std::env;
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, ErrorKind};
use std::process;

use serde::{Deserialize, Serialize};

use serenity::client::Client;
use serenity::model::{channel::GuildChannel, channel::Message, guild::GuildInfo, user::User};
use serenity::prelude::{Context, EventHandler, TypeMapKey};

struct NotifData;

impl TypeMapKey for NotifData {
    type Value = Vec<NotifChannel>;
}

#[derive(Serialize, Deserialize, Debug)]
struct NotifChannel {
    id: u64,
    subscribed_users: Vec<u64>,
}

struct Handler;

impl EventHandler for Handler {
    fn message(&self, ctx: Context, msg: Message) {
        println!("Received message from {}: {}", msg.author, msg.content);

        if msg.content.starts_with("!add-vc-notify") {
            handle_add_vc_notify(ctx, msg);
        } else if msg.content.starts_with("!remove-vc-notify") {
            handle_remove_vc_notify(ctx, msg);
        }
    }
}

fn handle_add_vc_notify(ctx: Context, msg: Message) {
    let mut data = ctx.data.write();
    let notif_data = data.get_mut::<NotifData>().unwrap();

    let author = &msg.author;
    let id: u64 = author.id.into();

    let channel = match get_channel_argument_from_msg(&msg) {
        Some(c) => c,
        None => {
            send_list_of_common_channels(&ctx, author);
            return;
        }
    };

    let channel_id = match channel.parse::<u64>() {
        Ok(id) => id,
        Err(_) => {
            send_reply(&ctx, author, "Not a valid channel ID!");
            return;
        }
    };

    let notif_channel = get_or_create_notif_channel(notif_data, channel_id);

    notif_channel.subscribed_users.push(id);

    send_reply(&ctx, author, "Subscribed to notifications for channel!");

    if let Err(err) = save_notif_data(notif_data) {
        println!("Error saving notif_data.json: {:?}", err);
    }
}

fn handle_remove_vc_notify(ctx: Context, msg: Message) {
    let mut data = ctx.data.write();
    let notif_data = data.get_mut::<NotifData>().unwrap();

    let author = &msg.author;
    let id: u64 = author.id.into();

    let channel = match get_channel_argument_from_msg(&msg) {
        Some(c) => c,
        None => {
            send_reply(&ctx, author, "Usage: `!remove-vc-notify <channel id>`!");
            return;
        }
    };

    let channel_id = match channel.parse::<u64>() {
        Ok(id) => id,
        Err(_) => {
            send_reply(&ctx, author, "Not a valid channel ID!");
            return;
        }
    };

    // TODO: Refactor all the `match { None => return }` to use an unwrap_ method instead.

    let notif_channel = match get_notif_channel(notif_data, channel_id) {
        None => {
            send_reply(&ctx, author, "You are not subscribed to this channel!");
            return;
        }
        Some(notif_channel) => notif_channel,
    };

    match notif_channel.subscribed_users.iter().position(|&u| u == id) {
        None => {
            send_reply(&ctx, author, "You are noit subscribed to this channel!");
            return;
        }
        Some(idx) => {
            notif_channel.subscribed_users.swap_remove(idx);
            send_reply(
                &ctx,
                author,
                "Unscribed from notifications for this channel!",
            );
        }
    }

    if let Err(err) = save_notif_data(notif_data) {
        println!("Error saving notif_data.json: {:?}", err);
    }
}

fn send_reply(ctx: &Context, recipient: &User, text: &str) {
    let dm = recipient.dm(ctx, |m| {
        m.content(text);

        m
    });

    if let Err(err) = dm {
        println!("Error sending DM to {}: {:?}", recipient, err);
    }
}

fn get_notif_channel(notif_data: &mut Vec<NotifChannel>, id: u64) -> Option<&mut NotifChannel> {
    notif_data.iter_mut().find(|c| c.id == id)
}

fn get_or_create_notif_channel(notif_data: &mut Vec<NotifChannel>, id: u64) -> &mut NotifChannel {
    let idx = notif_data
        .iter()
        .position(|c| c.id == id)
        .unwrap_or_else(|| {
            notif_data.push(NotifChannel {
                id: id,
                subscribed_users: vec![],
            });
            notif_data.len() - 1
        });
    &mut notif_data[idx]
}

fn get_channel_argument_from_msg(msg: &Message) -> Option<String> {
    let content = &msg.content;
    match content.find(' ') {
        None => None,
        Some(space_idx) => {
            if space_idx == content.len() {
                None
            } else {
                Some(content[(space_idx + 1)..].to_string())
            }
        }
    }
}

fn send_list_of_common_channels(ctx: &Context, user: &User) {
    match get_list_of_common_channels(ctx, user) {
        Ok(channels) => {
            let mut msg = ("Use `!add-vc-notify <channel id>` using one of the following channels:
[Server] Channel <channel id>")
                .to_string();

            for c in channels {
                msg.push_str(&format!("\n[{}] {} <{}>", c.0.name, c.1.name, c.1.id));
            }

            send_reply(ctx, user, &msg);
        }
        Err(err) => {
            send_reply(ctx, user, "Failed to find common channels!");
            println!("Error finding common channels: {:?}", err);
        }
    }
}

fn get_list_of_common_channels(
    ctx: &Context,
    user: &User,
) -> Result<Vec<(GuildInfo, GuildChannel)>, Box<dyn Error>> {
    let current_user = ctx.http.get_current_user()?;
    let current_guilds = current_user.guilds(&ctx.http)?;
    let mut common_channels = vec![];

    for guild in current_guilds {
        let is_guild_common =
            match ctx
                .http
                .get_guild_members(guild.id.into(), Some(1), Some(user.id.into()))
            {
                Ok(members) => members.len() > 0,
                Err(_) => false,
            };

        if is_guild_common {
            if let Ok(guild_channels) = ctx.http.get_channels(guild.id.into()) {
                common_channels.extend(guild_channels.into_iter().map(|c| (guild.clone(), c)));
            }
        }
    }

    Ok(common_channels)
}

fn main() {
    println!("Starting up...");

    let notif_data = load_notif_data().unwrap_or_else(|err| {
        println!("Error loading notif_data.json file: {:?}", err);
        process::exit(1)
    });

    println!(
        "Loaded subscription information for {} channels.",
        notif_data.len()
    );

    let mut client = Client::new(
        &env::var("PROBLEM_CHILD_TOKEN").expect("PROBLEM_CHILD_TOKEN"),
        Handler,
    )
    .expect("Error creating client");

    {
        let mut data = client.data.write();
        data.insert::<NotifData>(notif_data);
    }

    if let Err(err) = client.start() {
        println!("An error occured while running the client: {:?}", err);
    }
}

fn load_notif_data() -> Result<Vec<NotifChannel>, Box<dyn Error>> {
    match File::open("notif_data.json") {
        Err(err) => match err.kind() {
            // In case the file doesn't exist, just return an empty initial notifications list.
            ErrorKind::NotFound => {
                println!(
                    "notif_data.json file not found, proceeding with empty notifications list."
                );
                Ok(vec![])
            }
            // For any other errors, we should probably read the file but can't, so error out.
            _ => Err(Box::new(err)),
        },
        Ok(notif_file) => {
            // If the file can be read fine, parse it into a users list.
            // If any errors occur here, those are fatal, just pass them up.
            let reader = BufReader::new(notif_file);
            let notif_data = serde_json::from_reader(reader)?;
            Ok(notif_data)
        }
    }
}

fn save_notif_data(notif_data: &Vec<NotifChannel>) -> Result<(), Box<dyn Error>> {
    match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("notif_data.json")
    {
        Err(err) => Err(Box::new(err)),
        Ok(notif_file) => {
            let writer = BufWriter::new(notif_file);
            if let Err(err) = serde_json::to_writer_pretty(writer, notif_data) {
                Err(Box::new(err))
            } else {
                Ok(())
            }
        }
    }
}
