use serde::{Deserialize, Serialize};
use serenity::model::id::{ChannelId, GuildId, UserId};

#[derive(Serialize, Deserialize, Debug)]
pub struct PCData {
    pub guilds: Vec<PCGuild>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PCGuild {
    pub id: u64,
    admins: Vec<AdminUser>,
    afk_channels: Vec<u64>,
    notif_channels: Vec<PCNotifChannel>,
}

#[derive(Serialize, Deserialize, Debug)]
struct AdminUser {
    id: u64,
    send_notif_copies: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PCNotifChannel {
    pub id: u64,
    pub subscribed_users: Vec<u64>,
}

impl PCData {
    pub fn default() -> PCData {
        PCData { guilds: vec![] }
    }

    pub fn find_subscribed_users<'a>(
        &'a self,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> Option<impl Iterator<Item = UserId> + 'a> {
        self.guilds
            .iter()
            .find(|g| g.id == guild_id.0)
            .and_then(|guild| guild.notif_channels.iter().find(|c| c.id == channel_id.0))
            .map(|channel| channel.subscribed_users.iter().map(|id| UserId::from(*id)))
    }

    pub fn is_afk_channel(&self, guild_id: GuildId, channel_id: ChannelId) -> bool {
        self.guilds
            .iter()
            .find(|g| g.id == guild_id.0)
            .map(|guild| guild.afk_channels.iter().any(|&c| c == channel_id.0))
            .unwrap_or(false)
    }

    pub fn add_subscription(&mut self, user_id: UserId, guild_id: GuildId, channel_id: ChannelId) {
        let guild = Self::find_or_insert(
            &mut self.guilds,
            |g| g.id == guild_id.0,
            PCGuild::new(guild_id),
        );

        let notif_channel = Self::find_or_insert(
            &mut guild.notif_channels,
            |c| c.id == channel_id.0,
            PCNotifChannel::new(channel_id),
        );

        Self::insert_if_not_exists(&mut notif_channel.subscribed_users, user_id.0);
    }

    pub fn remove_subscription(
        &mut self,
        user_id: UserId,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> bool {
        let guild = match self.guilds.iter_mut().find(|g| g.id == guild_id.0) {
            Some(g) => g,
            None => return false,
        };

        let notif_channel = match guild
            .notif_channels
            .iter_mut()
            .find(|c| c.id == channel_id.0)
        {
            Some(c) => c,
            None => return false,
        };

        let index = match notif_channel
            .subscribed_users
            .iter()
            .position(|&u| u == user_id.0)
        {
            Some(i) => i,
            None => return false,
        };

        notif_channel.subscribed_users.swap_remove(index);
        true
    }

    pub fn is_admin(&self, user_id: UserId, guild_id: GuildId) -> bool {
        let guild = match self.guilds.iter().find(|g| g.id == guild_id.0) {
            Some(g) => g,
            None => return false,
        };
        guild.admins.iter().any(|u| u.id == user_id.0)
    }

    pub fn should_send_notif_copies(&self, joined_user_id: UserId, guild_id: GuildId) -> bool {
        let guild = match self.guilds.iter().find(|g| g.id == guild_id.0) {
            Some(g) => g,
            None => return false,
        };
        guild
            .admins
            .iter()
            .find(|u| u.id == joined_user_id.0)
            .map(|u| u.send_notif_copies)
            .unwrap_or(false)
    }

    pub fn add_afk_channel(&mut self, guild_id: GuildId, channel_id: ChannelId) {
        let guild = Self::find_or_insert(
            &mut self.guilds,
            |g| g.id == guild_id.0,
            PCGuild::new(guild_id),
        );

        Self::insert_if_not_exists(&mut guild.afk_channels, channel_id.0);
    }

    pub fn remove_afk_channel(&mut self, guild_id: GuildId, channel_id: ChannelId) -> bool {
        let guild = match self.guilds.iter_mut().find(|g| g.id == guild_id.0) {
            Some(g) => g,
            None => return false,
        };

        let index = match guild.afk_channels.iter().position(|&c| c == channel_id.0) {
            Some(i) => i,
            None => return false,
        };

        guild.afk_channels.swap_remove(index);
        true
    }

    fn find_or_insert<T, P>(vec: &mut Vec<T>, predicate: P, default: T) -> &mut T
    where
        P: FnMut(&T) -> bool,
    {
        let idx = vec.iter().position(predicate).unwrap_or_else(|| {
            vec.push(default);
            vec.len() - 1
        });

        &mut vec[idx]
    }

    fn insert_if_not_exists<T>(vec: &mut Vec<T>, val: T)
    where
        T: PartialEq,
    {
        match vec.iter().find(|&v| *v == val) {
            Some(_) => (),
            None => vec.push(val),
        }
    }
}

impl PCGuild {
    fn new(id: GuildId) -> PCGuild {
        PCGuild {
            id: id.0,
            admins: vec![],
            afk_channels: vec![],
            notif_channels: vec![],
        }
    }
}

impl PCNotifChannel {
    fn new(id: ChannelId) -> PCNotifChannel {
        PCNotifChannel {
            id: id.0,
            subscribed_users: vec![],
        }
    }
}
