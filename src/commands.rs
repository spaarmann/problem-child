use crate::model::NotifChannel;
use crate::storage;

use std::error::Error;

use serenity::model::{
    channel::{ChannelType, GuildChannel, Message},
    event::ResumedEvent,
    gateway::Ready,
    guild::GuildInfo,
    id::{ChannelId, GuildId, UserId},
    user::{CurrentUser, OnlineStatus, User},
    voice::VoiceState,
};

use serenity::prelude::{Context, EventHandler, TypeMapKey};

pub struct NotifData;

impl TypeMapKey for NotifData {
    type Value = Vec<NotifChannel>;
}

pub struct Handler;

impl EventHandler for Handler {
    fn message(&self, ctx: Context, msg: Message) {
        if msg.is_own(&ctx.cache) {
            return;
        }

        if !msg.is_private() {
            return;
        }

        println!("[message] {}: {}", msg.author, msg.content);

        if msg.content.starts_with("!add-vc-notify") {
            handle_add_vc_notify(&ctx, msg);
        } else if msg.content.starts_with("!remove-vc-notify") {
            handle_remove_vc_notify(&ctx, msg);
        } else if msg.content.starts_with("!help") {
            handle_help(&ctx, msg);
        } else {
            // For an unknown command, also print help for now.
            handle_help(&ctx, msg);
        }
    }

    fn voice_state_update(
        &self,
        ctx: Context,
        _guild: Option<GuildId>,
        old: Option<VoiceState>,
        new: VoiceState,
    ) {
        println!(
            "[voice_state_update] {}: {}",
            new.user_id,
            new.channel_id.unwrap_or(ChannelId::from(0))
        );

        if !is_join_event(&old, &new) {
            return;
        }

        send_notifications(&ctx, &new);
    }

    fn cache_ready(&self, _ctx: Context, _guilds: Vec<GuildId>) {
        println!("[cache_ready]");
    }

    fn guild_unavailable(&self, _ctx: Context, guild_id: GuildId) {
        println!("[guild_unavailable] {}", guild_id);
    }

    fn ready(&self, _ctx: Context, data_about_bot: Ready) {
        println!(
            "[ready] {} (v{})",
            data_about_bot.session_id, data_about_bot.version
        );
    }

    fn resume(&self, _ctx: Context, _: ResumedEvent) {
        println!("[resume]");
    }

    fn user_update(&self, _ctx: Context, _old_data: CurrentUser, _new: CurrentUser) {
        println!("[user_update]");
    }

    fn unknown(&self, _ctx: Context, name: String, _raw: serde_json::Value) {
        println!("[unknown]: {}", name);
    }
}

fn is_join_event(old: &Option<VoiceState>, new_state: &VoiceState) -> bool {
    let old_state = match old {
        None => return true,
        Some(o) => o,
    };

    let old_channel = match old_state.channel_id {
        None => return true,
        Some(id) => id,
    };

    let new_channel = match new_state.channel_id {
        None => return false,
        Some(id) => id,
    };

    old_channel != new_channel
}

fn handle_help(ctx: &Context, msg: Message) {
    send_msg(
        ctx,
        &msg.author,
        concat!(
            "Hello! I currently support two commands:\n",
            "- `!add-vc-notify`\n",
            "- `!remove-vc-notify\n`",
            "Send any command by itself to get more information!"
        ),
    );
}

fn send_notifications(ctx: &Context, voice_state: &VoiceState) {
    let channel_id = match voice_state.channel_id {
        None => return,
        Some(id) => id,
    };

    let mut data = ctx.data.write();
    let notif_data = data.get_mut::<NotifData>().unwrap();

    let notif_channel = match get_notif_channel(notif_data, channel_id.into()) {
        None => return,
        Some(c) => c,
    };

    let channel = match channel_id.to_channel(&ctx.http) {
        Err(_) => return,
        Ok(c) => c,
    };

    let guild_channel_lock = match channel.guild() {
        None => return,
        Some(g) => g,
    };
    let guild_channel = guild_channel_lock.read();

    let channel_members = guild_channel.members(&ctx.cache).unwrap_or_else(|e| {
        println!("Failed to get members in channel: {:?}", e);
        vec![]
    });

    let guild_lock = match guild_channel.guild(&ctx.cache) {
        None => return,
        Some(g) => g,
    };

    let joined_user_name = voice_state
        .user_id
        .to_user(&ctx.http)
        .map(|u| u.name)
        .unwrap_or("Someone".to_string());

    let guild = guild_lock.read();

    for uid in &notif_channel.subscribed_users {
        let user_id = UserId::from(*uid);

        if user_id == voice_state.user_id {
            // Don't notify users that they joined themselves.
            continue;
        }

        if channel_members.iter().any(|m| m.user_id() == user_id) {
            // Don't notify users if they are already in the voice channel themselves.
            continue;
        }

        let presence = match guild.presences.get(&user_id) {
            None => continue,
            Some(p) => p,
        };

        let send_notif = match presence.status {
            OnlineStatus::Online => true,
            OnlineStatus::Idle => true,
            OnlineStatus::DoNotDisturb => false,
            OnlineStatus::Invisible => false,
            OnlineStatus::Offline => false,
            _ => false,
        };

        if send_notif {
            let user = match user_id.to_user(&ctx.http) {
                Err(_) => continue,
                Ok(u) => u,
            };

            send_msg(
                &ctx,
                &user,
                &format!(
                    "{} joined {} on {}!",
                    joined_user_name, guild_channel.name, guild.name
                ),
            );
        }
    }
}

fn handle_add_vc_notify(ctx: &Context, msg: Message) {
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
            send_msg(&ctx, author, "Not a valid channel ID!");
            return;
        }
    };

    let notif_channel = get_or_create_notif_channel(notif_data, channel_id);

    notif_channel.subscribed_users.push(id);

    send_msg(&ctx, author, "Subscribed to notifications for channel!");

    if let Err(err) = storage::save_notif_data(notif_data) {
        println!("Error saving notif_data.json: {:?}", err);
    }
}

fn handle_remove_vc_notify(ctx: &Context, msg: Message) {
    let mut data = ctx.data.write();
    let notif_data = data.get_mut::<NotifData>().unwrap();

    let author = &msg.author;
    let id: u64 = author.id.into();

    let channel = match get_channel_argument_from_msg(&msg) {
        Some(c) => c,
        None => {
            send_msg(&ctx, author, "Usage: `!remove-vc-notify <channel id>`!");
            return;
        }
    };

    let channel_id = match channel.parse::<u64>() {
        Ok(id) => id,
        Err(_) => {
            send_msg(&ctx, author, "Not a valid channel ID!");
            return;
        }
    };

    let notif_channel = match get_notif_channel(notif_data, channel_id) {
        None => {
            send_msg(&ctx, author, "You are not subscribed to this channel!");
            return;
        }
        Some(notif_channel) => notif_channel,
    };

    match notif_channel.subscribed_users.iter().position(|&u| u == id) {
        None => {
            send_msg(&ctx, author, "You are noit subscribed to this channel!");
            return;
        }
        Some(idx) => {
            notif_channel.subscribed_users.swap_remove(idx);
            send_msg(
                &ctx,
                author,
                "Unscribed from notifications for this channel!",
            );
        }
    }

    if let Err(err) = storage::save_notif_data(notif_data) {
        println!("Error saving notif_data.json: {:?}", err);
    }
}

fn send_msg(ctx: &Context, recipient: &User, text: &str) {
    let dm = recipient.dm(ctx, |m| {
        m.content(text);

        m
    });

    if let Err(err) = dm {
        println!("Error sending DM to {}: {:?}", recipient, err);
    }
}

fn get_channel_argument_from_msg(msg: &Message) -> Option<String> {
    let content = &msg.content;
    content
        .trim()
        .find(' ')
        .map(|space_idx| content[(space_idx + 1)..].to_string())
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

            send_msg(ctx, user, &msg);
        }
        Err(err) => {
            send_msg(ctx, user, "Failed to find common channels!");
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
        let is_guild_common = ctx
            .http
            .get_guild_members(guild.id.into(), Some(1), Some(user.id.into()))
            .map_or(false, |members| members.len() > 0);

        if is_guild_common {
            if let Ok(guild_channels) = ctx.http.get_channels(guild.id.into()) {
                common_channels.extend(guild_channels.into_iter().map(|c| (guild.clone(), c)));
            }
        }
    }

    Ok(common_channels
        .into_iter()
        .filter(|(_, c)| c.kind == ChannelType::Voice)
        .collect())
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
