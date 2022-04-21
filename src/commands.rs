use crate::model::PCData;
use crate::storage;

use log::{debug, error, info, warn};

use serenity::{
    async_trait,
    model::{
        channel::{Channel, ChannelType, GuildChannel, Message},
        event::ResumedEvent,
        gateway::Ready,
        guild::{Guild, GuildInfo},
        id::{ChannelId, GuildId, UserId},
        user::{CurrentUser, OnlineStatus, User},
        voice::VoiceState,
    },
    prelude::{Context, EventHandler, TypeMapKey},
};

pub struct DataKey;

impl TypeMapKey for DataKey {
    type Value = PCData;
}

pub struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, mut msg: Message) {
        debug!("Message from {}", msg.author);

        if msg.is_own(&ctx.cache) {
            return;
        }

        if !msg.is_private() {
            suppress_embeds_if_necessary(&ctx, &mut msg).await;
            return;
        }

        info!("Handling message: {}: {}", msg.author, msg.content);

        if msg.content.starts_with("!add-vc-notify") {
            handle_add_vc_notify(&ctx, msg).await;
        } else if msg.content.starts_with("!remove-vc-notify") {
            handle_remove_vc_notify(&ctx, msg).await;
        } else if msg.content.starts_with("!add-afk-channel") {
            handle_add_afk_channel(&ctx, msg).await;
        } else if msg.content.starts_with("!remove-afk-channel") {
            handle_remove_afk_channel(&ctx, msg).await;
        } else {
            // !help, or an unknown command, also print help for now.
            handle_help(&ctx, msg).await;
        }
    }

    async fn voice_state_update(&self, ctx: Context, old: Option<VoiceState>, new: VoiceState) {
        info!(
            "[voice_state_update] {}: {}",
            new.user_id,
            new.channel_id.unwrap_or_else(|| ChannelId::from(0))
        );

        if !is_join_event(&ctx, &old, &new).await {
            debug!("[voice_state_update] Not sending notifs because !is_join_event.");
            return;
        }

        send_notifications(&ctx, &new).await;
    }

    async fn cache_ready(&self, _ctx: Context, _guilds: Vec<GuildId>) {
        info!("[cache_ready]");
    }

    async fn guild_unavailable(&self, _ctx: Context, guild_id: GuildId) {
        info!("[guild_unavailable] {}", guild_id);
    }

    async fn ready(&self, _ctx: Context, data_about_bot: Ready) {
        info!(
            "[ready] {} (v{})",
            data_about_bot.session_id, data_about_bot.version
        );
    }

    async fn resume(&self, _ctx: Context, _: ResumedEvent) {
        info!("[resume]");
    }

    async fn user_update(&self, _ctx: Context, _old_data: CurrentUser, _new: CurrentUser) {
        info!("[user_update]");
    }

    async fn unknown(&self, _ctx: Context, name: String, _raw: serde_json::Value) {
        info!("[unknown]: {}", name);
    }
}

async fn suppress_embeds_if_necessary(ctx: &Context, msg: &mut Message) {
    const SUPPRESSED_EMBDS: [&'static str; 4] = [
        "https://heardle.app/",
        "https://framed.wtf/",
        "https://moviedle.app/",
        "https://oec.world/en/tradle",
    ];

    debug!(
        "Checking whether embed should be suppressed: {:?}",
        msg.embeds
    );

    if let [embed] = &msg.embeds[..] {
        if let Some(url) = embed.url.as_deref() {
            if SUPPRESSED_EMBDS.contains(&url) {
                if let Err(e) = msg.suppress_embeds(&ctx.http).await {
                    error!("Error suppressing embed: {:?}", e);
                }
            }
        }
    }
}

async fn is_join_event(ctx: &Context, old: &Option<VoiceState>, new_state: &VoiceState) -> bool {
    // If there is no new channel, this is by definition not a join event.
    let new_channel = match new_state.channel_id {
        None => return false,
        Some(id) => id,
    };

    let guild = match get_guild_from_channel(ctx, new_channel).await {
        None => return false,
        Some(g) => g,
    };
    let data = ctx.data.read().await;
    let pc_data = data.get::<DataKey>().unwrap();

    // If the new channels is an AFK channel, this shouldn't count as a join event.
    if pc_data.is_afk_channel(guild, new_channel) {
        return false;
    }

    // If there is no old state, but the user joined a non-AFK channel, this is a join event.
    let old_state = match old {
        None => return true,
        Some(o) => o,
    };

    // Same for there not being an old channel.
    let old_channel = match old_state.channel_id {
        None => return true,
        Some(id) => id,
    };

    // If the old channel is an AFK channel, this should count as joining.
    if pc_data.is_afk_channel(guild, old_channel) {
        return true;
    }

    // Otherwise, if there is an old non-AFK channel, it does not count.
    false
}

async fn handle_help(ctx: &Context, msg: Message) {
    send_msg(
        ctx,
        &msg.author,
        concat!(
            "Hello! I currently support two commands:\n",
            "- `!add-vc-notify`\n",
            "- `!remove-vc-notify\n`",
            "Send any command by itself to get more information!"
        ),
    )
    .await;
}

async fn send_notifications(ctx: &Context, voice_state: &VoiceState) {
    let channel_id = match voice_state.channel_id {
        None => return,
        Some(id) => id,
    };

    let data = ctx.data.read().await;
    let pc_data = data.get::<DataKey>().unwrap();

    let channel = match channel_id.to_channel(&ctx.http).await {
        Err(_) => return,
        Ok(c) => c,
    };

    let guild_channel = match channel.guild() {
        None => return,
        Some(g) => g,
    };

    let channel_members = guild_channel.members(&ctx.cache).await.unwrap_or_else(|e| {
        warn!("Failed to get members in channel: {:?}", e);
        vec![]
    });

    let guild = match guild_channel.guild(&ctx.cache) {
        None => return,
        Some(g) => g,
    };

    let joined_user = voice_state.user_id.to_user(&ctx.http).await.ok();

    let joined_user_name = joined_user
        .clone()
        .map(|u| u.name)
        .unwrap_or_else(|| "Someone".to_string());

    let mut notified_users = Vec::new();

    debug!(
        "[send_notifications] Determining notifs for {:?} having joined {:?}",
        joined_user, guild_channel
    );

    let subscribed_users = pc_data.find_subscribed_users(guild.id, guild_channel.id);
    if let Some(subscribed_users) = subscribed_users {
        'user: for user_id in subscribed_users {
            debug!("Testing {:?} from subscribed_users", user_id);
            if user_id == voice_state.user_id {
                // Don't notify users that they joined themselves.
                debug!("Not notifying {:?} because they are the joiner.", user_id);
                continue;
            }

            if channel_members.iter().any(|m| m.user.id == user_id) {
                // Don't notify users if they are already in the voice channel themselves.
                debug!(
                    "Not notifying {:?} because they are in the channel.",
                    user_id
                );
                continue;
            }

            // TODO: Is there no better way of determining this?
            // TODO: This was a bit prettier as an iterator, but not sure how that plays with async
            for (_, c) in guild.channels.iter() {
                if let Channel::Guild(c) = c {
                    if skip_because_in_channel(ctx, c, &guild, user_id, pc_data).await {
                        debug!(
                            "Not notifying {:?} because they are in another non-AFK channel.",
                            user_id
                        );
                        continue 'user;
                    }
                }
            }

            let presence = match guild.presences.get(&user_id) {
                None => {
                    debug!(
                        "Not notifying {:?} because their presence is None.",
                        user_id
                    );
                    continue;
                }
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
                let user = match user_id.to_user(&ctx.http).await {
                    Err(e) => {
                        debug!(
                            "Not notifying {:?} because they could not be turned into a User: {:?}",
                            user_id, e
                        );
                        continue;
                    }
                    Ok(u) => u,
                };

                send_msg(
                    &ctx,
                    &user,
                    &format!(
                        "{} joined {} on {}!",
                        joined_user_name, guild_channel.name, guild.name
                    ),
                )
                .await;

                notified_users.push(user);
            } else {
                debug!(
                    "Not notifying {:?} because send_notif is false with presence.status {:?}",
                    user_id, presence.status
                );
            }
        }

        if let Some(joined_user) = joined_user {
            if pc_data.should_send_notif_copies(joined_user.id, guild.id) {
                let user_list = match notified_users {
                    _ if notified_users.is_empty() => "nobody".to_string(),
                    notified_users => notified_users
                        .into_iter()
                        .map(|u| u.name)
                        .collect::<Vec<_>>()
                        .join(", "),
                };

                send_msg(
                    &ctx,
                    &joined_user,
                    &format!("Sent join notifications to {}!", user_list),
                )
                .await;
            }
        }
    }
}

// Don't notify users if they are already in *any* voice channel on the same
// server, unless it's an AFK channel.
async fn skip_because_in_channel(
    ctx: &Context,
    channel: &GuildChannel,
    guild: &Guild,
    user_id: UserId,
    pc_data: &PCData,
) -> bool {
    let k = channel.kind == ChannelType::Voice;
    let afk = pc_data.is_afk_channel(guild.id, channel.id);
    let member = is_member(channel, user_id, ctx).await;
    k && !afk && member
}

async fn handle_add_vc_notify(ctx: &Context, msg: Message) {
    let mut data = ctx.data.write().await;
    let pc_data = data.get_mut::<DataKey>().unwrap();

    let author = &msg.author;
    let id = author.id;

    let channel = match get_channel_from_msg(&ctx, &msg).await {
        Some(c) => c,
        None => return,
    };
    let guild_channel = match channel.guild() {
        Some(gc) => gc,
        None => {
            send_msg(
                &ctx,
                author,
                "Could not find server that the channel belongs to!",
            )
            .await;
            return;
        }
    };

    let guild_name = guild_channel
        .guild(&ctx.cache)
        .map(|g| g.name)
        .unwrap_or_else(|| "<error fetching server name>".to_string());

    pc_data.add_subscription(id, guild_channel.guild_id, guild_channel.id);

    send_msg(
        &ctx,
        author,
        &format!(
            "Subscribed to notifications for {} on {}!",
            guild_channel.name, guild_name
        ),
    )
    .await;

    if let Err(err) = storage::save_data(pc_data) {
        error!("Error saving notif_data.json: {:?}", err);
    }
}

async fn handle_remove_vc_notify(ctx: &Context, msg: Message) {
    let mut data = ctx.data.write().await;
    let pc_data = data.get_mut::<DataKey>().unwrap();

    let author = &msg.author;
    let id = author.id;

    let channel = match get_channel_from_msg(&ctx, &msg).await {
        Some(c) => c,
        None => return,
    };

    let guild_channel = match channel.guild() {
        Some(gc) => gc,
        None => {
            send_msg(
                &ctx,
                author,
                "Could not find server that the channel belongs to!",
            )
            .await;
            return;
        }
    };

    if pc_data.remove_subscription(id, guild_channel.guild_id, guild_channel.id) {
        send_msg(
            &ctx,
            author,
            "Unscribed from notifications for this channel!",
        )
        .await;
    } else {
        send_msg(&ctx, author, "You are not subscribed to this channel!").await;
    }

    if let Err(err) = storage::save_data(pc_data) {
        error!("Error saving notif_data.json: {:?}", err);
    }
}

async fn handle_add_afk_channel(ctx: &Context, msg: Message) {
    let mut data = ctx.data.write().await;
    let pc_data = data.get_mut::<DataKey>().unwrap();

    let author = &msg.author;
    let id = author.id;

    let channel = match get_channel_from_msg(&ctx, &msg).await {
        Some(c) => c,
        None => return,
    };

    let guild_channel = match channel.guild() {
        Some(gc) => gc,
        None => {
            send_msg(
                &ctx,
                author,
                "Could not find server that the channel belongs to!",
            )
            .await;
            return;
        }
    };

    if !pc_data.is_admin(id, guild_channel.guild_id) {
        send_msg(
            &ctx,
            author,
            "You are not permitted to modify administrative settings for this server!",
        )
        .await;
        return;
    }

    pc_data.add_afk_channel(guild_channel.guild_id, guild_channel.id);
    send_msg(&ctx, author, "Set channel as AFK channel!").await;

    if let Err(err) = storage::save_data(pc_data) {
        error!("Error saving notif_data.json: {:?}", err);
    }
}

async fn handle_remove_afk_channel(ctx: &Context, msg: Message) {
    let mut data = ctx.data.write().await;
    let pc_data = data.get_mut::<DataKey>().unwrap();

    let author = &msg.author;
    let id = author.id;

    let channel = match get_channel_from_msg(&ctx, &msg).await {
        Some(c) => c,
        None => return,
    };

    let guild_channel = match channel.guild() {
        Some(gc) => gc,
        None => {
            send_msg(
                &ctx,
                author,
                "Could not find server that the channel belongs to!",
            )
            .await;
            return;
        }
    };

    if !pc_data.is_admin(id, guild_channel.guild_id) {
        send_msg(
            &ctx,
            author,
            "You are not permitted to modify administrative settings for this server!",
        )
        .await;
        return;
    }

    if pc_data.remove_afk_channel(guild_channel.guild_id, guild_channel.id) {
        send_msg(&ctx, author, "Unset channel as AFK channel!").await;
    } else {
        send_msg(
            &ctx,
            author,
            "Could not unset as AFK channel. Is the channel currently an AFK channel?",
        )
        .await;
    }

    if let Err(err) = storage::save_data(pc_data) {
        error!("Error saving notif_data.json: {:?}", err);
    }
}

async fn send_msg(ctx: &Context, recipient: &User, text: &str) {
    let dm = recipient
        .dm(ctx, |m| {
            m.content(text);

            m
        })
        .await;

    if let Err(err) = dm {
        warn!("Error sending DM to {}: {:?}", recipient, err);
    }
}

fn get_channel_argument_from_msg(msg: &Message) -> Option<String> {
    let content = &msg.content;
    content
        .trim()
        .find(' ')
        .map(|space_idx| content[(space_idx + 1)..].to_string())
}

async fn get_channel_from_msg(ctx: &Context, msg: &Message) -> Option<Channel> {
    let author = &msg.author;

    let channel = match get_channel_argument_from_msg(&msg) {
        Some(c) => c,
        None => {
            send_list_of_common_channels(&ctx, author).await;
            return None;
        }
    };

    let channel_id = match channel.parse::<u64>() {
        Ok(id) => id,
        Err(_) => {
            send_msg(&ctx, author, "Not a valid channel ID!").await;
            return None;
        }
    };

    let channel = match ctx.http.get_channel(channel_id).await {
        Ok(c) => c,
        Err(_) => {
            send_msg(&ctx, author, "Could not find channel!").await;
            return None;
        }
    };

    Some(channel)
}

async fn get_guild_from_channel(ctx: &Context, channel: ChannelId) -> Option<GuildId> {
    ctx.http
        .get_channel(channel.into())
        .await
        .ok()
        .and_then(|channel| channel.guild())
        .map(|guild| guild.guild_id)
}

async fn send_list_of_common_channels(ctx: &Context, user: &User) {
    match get_list_of_common_channels(ctx, user).await {
        Ok(channels) => {
            let mut msg = ("Use `!add-vc-notify <channel id>` using one of the following channels:
                            [Server] Channel <channel id>")
                .to_string();

            for c in channels {
                msg.push_str(&format!("\n[{}] {} <{}>", c.0.name, c.1.name, c.1.id));
            }

            send_msg(ctx, user, &msg).await;
        }
        Err(err) => {
            warn!("Error finding common channels: {:?}", err);
            drop(err);

            send_msg(ctx, user, "Failed to find common channels!").await;
        }
    }
}

async fn get_list_of_common_channels(
    ctx: &Context,
    user: &User,
) -> serenity::Result<Vec<(GuildInfo, GuildChannel)>> {
    let current_user = ctx.http.get_current_user().await?;
    let current_guilds = current_user.guilds(&ctx.http).await?;
    let mut common_channels = vec![];

    for guild in current_guilds {
        let is_guild_common = ctx
            .http
            .get_guild_members(guild.id.into(), Some(1), Some(user.id.into()))
            .await
            .map_or(false, |members| !members.is_empty());

        if is_guild_common {
            if let Ok(guild_channels) = ctx.http.get_channels(guild.id.into()).await {
                common_channels.extend(guild_channels.into_iter().map(|c| (guild.clone(), c)));
            }
        }
    }

    Ok(common_channels
        .into_iter()
        .filter(|(_, c)| c.kind == ChannelType::Voice)
        .collect())
}

async fn is_member(channel: &GuildChannel, user_id: UserId, ctx: &Context) -> bool {
    channel
        .members(&ctx.cache)
        .await
        .map(|members| members.into_iter().any(|u| u.user.id == user_id))
        .unwrap_or(false)
}
