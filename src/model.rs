use serde::{Deserialize, Serialize};

// TODO: Might be nice if the IDs here were serenity's Id types instead of all u64s,
// but that requires figuring out how serialization works in that case.

#[derive(Serialize, Deserialize, Debug)]
pub struct PCData {
    pub guilds: Vec<PCGuild>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PCGuild {
    pub id: u64,
    pub admins: Vec<u64>,
    pub afk_channels: Vec<u64>,
    pub notif_channels: Vec<PCNotifChannel>,
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

    pub fn find_subscribed_users(&self, guild_id: u64, channel_id: u64) -> Option<&Vec<u64>> {
        self.guilds
            .iter()
            .find(|g| g.id == guild_id)
            .and_then(|guild| guild.notif_channels.iter().find(|c| c.id == channel_id))
            .map(|channel| &channel.subscribed_users)
    }

    pub fn add_subscription(&mut self, user_id: u64, guild_id: u64, channel_id: u64) {
        let guild = Self::find_or_insert(
            &mut self.guilds,
            |g| g.id == guild_id,
            PCGuild::new(guild_id),
        );

        let notif_channel = Self::find_or_insert(
            &mut guild.notif_channels,
            |c| c.id == channel_id,
            PCNotifChannel::new(channel_id),
        );

        Self::insert_if_not_exists(&mut notif_channel.subscribed_users, user_id);
    }

    pub fn remove_subscription(&mut self, user_id: u64, guild_id: u64, channel_id: u64) -> bool {
        let guild = match self.guilds.iter_mut().find(|g| g.id == guild_id) {
            Some(g) => g,
            None => return false,
        };

        let notif_channel = match guild.notif_channels.iter_mut().find(|c| c.id == channel_id) {
            Some(c) => c,
            None => return false,
        };

        let index = match notif_channel
            .subscribed_users
            .iter()
            .position(|&u| u == user_id)
        {
            Some(i) => i,
            None => return false,
        };

        notif_channel.subscribed_users.swap_remove(index);
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
    fn new(id: u64) -> PCGuild {
        PCGuild {
            id: id,
            admins: vec![],
            afk_channels: vec![],
            notif_channels: vec![],
        }
    }
}

impl PCNotifChannel {
    fn new(id: u64) -> PCNotifChannel {
        PCNotifChannel {
            id: id,
            subscribed_users: vec![],
        }
    }
}
