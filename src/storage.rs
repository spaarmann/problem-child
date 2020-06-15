use crate::model::NotifChannel;

use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, ErrorKind};

pub fn load_notif_data() -> Result<Vec<NotifChannel>, Box<dyn Error>> {
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

pub fn save_notif_data(notif_data: &Vec<NotifChannel>) -> Result<(), Box<dyn Error>> {
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
