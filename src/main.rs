use std::env;
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, ErrorKind};
use std::process;

use serde::{Deserialize, Serialize};

use serenity::client::Client;
use serenity::model::{channel::GuildChannel, channel::Message, guild::GuildInfo, user::User};
use serenity::prelude::{Context, EventHandler, TypeMapKey};

struct UserData;

impl TypeMapKey for UserData {
    type Value = Vec<PCUser>;
}

#[derive(Serialize, Deserialize, Debug)]
struct PCUser {
    id: u64,
    subscribed_channels: Vec<PCChannel>,
}

#[derive(Serialize, Deserialize, Debug)]
struct PCChannel {
    id: u64,
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
    let users = data.get_mut::<UserData>().unwrap();

    let author = &msg.author;
    let id: u64 = author.id.into();

    let user = get_or_create_user(users, id);

    // Check if msg contains a channel argument.
    // If so,
    //   Add channel to list of subscribed channels.
    // If not,
    //   Find guilds shared by the bot and the user,
    //   send back a list of possible channels to subscribe to.

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

    user.subscribed_channels.push(PCChannel { id: channel_id });

    if let Err(err) = save_users(users) {
        println!("Error saving users.json: {:?}", err);
    }
}

fn handle_remove_vc_notify(ctx: Context, msg: Message) {
    let mut data = ctx.data.write();
    let users = data.get_mut::<UserData>().unwrap();

    let author = &msg.author;
    let id: u64 = author.id.into();

    let user = match get_user(users, id) {
        Some(user) => user,
        None => {
            send_reply(
                &ctx,
                &msg.author,
                "You are not subscribed to any voice channels!",
            );
            return;
        }
    };
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

fn get_user(users: &mut Vec<PCUser>, id: u64) -> Option<&mut PCUser> {
    users.iter_mut().find(|u| u.id == id)
}

fn get_or_create_user(users: &mut Vec<PCUser>, id: u64) -> &mut PCUser {
    let idx = users.iter().position(|u| u.id == id).unwrap_or_else(|| {
        users.push(PCUser {
            id: id,
            subscribed_channels: vec![],
        });
        users.len() - 1
    });
    &mut users[idx]
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

    let registered_users = load_users().unwrap_or_else(|err| {
        println!("Error loading users.json file: {:?}", err);
        process::exit(1)
    });

    println!("Loaded {} users from users.json.", registered_users.len());

    let mut client = Client::new(
        &env::var("PROBLEM_CHILD_TOKEN").expect("PROBLEM_CHILD_TOKEN"),
        Handler,
    )
    .expect("Error creating client");

    {
        let mut data = client.data.write();
        data.insert::<UserData>(registered_users);
    }

    if let Err(err) = client.start() {
        println!("An error occured while running the client: {:?}", err);
    }
}

fn load_users() -> Result<Vec<PCUser>, Box<dyn Error>> {
    match File::open("users.json") {
        Err(err) => match err.kind() {
            // In case the file doesn't exist, just return an empty initial users list.
            ErrorKind::NotFound => {
                println!("users.json file not found, proceeding with empty users list.");
                Ok(vec![])
            }
            // For any other errors, we should probably read the file but can't, so error out.
            _ => Err(Box::new(err)),
        },
        Ok(users_file) => {
            // If the file can be read fine, parse it into a users list.
            // If any errors occur here, those are fatal, just pass them up.
            let reader = BufReader::new(users_file);
            let users = serde_json::from_reader(reader)?;
            Ok(users)
        }
    }
}

fn save_users(users: &Vec<PCUser>) -> Result<(), Box<dyn Error>> {
    match OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("users.json")
    {
        Err(err) => Err(Box::new(err)),
        Ok(users_file) => {
            let writer = BufWriter::new(users_file);
            if let Err(err) = serde_json::to_writer_pretty(writer, users) {
                Err(Box::new(err))
            } else {
                Ok(())
            }
        }
    }
}
