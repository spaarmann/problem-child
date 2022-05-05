use crate::model::PCData;

use log::info;
use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, ErrorKind};

// Simply loads configuration data from config/pc_data.json relative to the current workding
// directory.
// If the file doesn't exist, returns a default configuration instead.
// On any other errors, returns the error instead.
pub fn load_data() -> Result<PCData, Box<dyn Error>> {
    match File::open("config/pc_data.json") {
        Ok(file) => {
            let reader = BufReader::new(file);
            let data = serde_json::from_reader(reader)?;
            Ok(data)
        }
        Err(err) => match err.kind() {
            ErrorKind::NotFound => {
                info!("config/pc_data.json file not found, proceeding with new default data.");
                Ok(PCData::default())
            }
            _ => Err(Box::new(err)),
        },
    }
}

pub fn save_data(data: &PCData) -> Result<(), Box<dyn Error>> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open("config/pc_data.json")
        .and_then(|file| {
            let writer = BufWriter::new(file);
            serde_json::to_writer_pretty(writer, data).map_err(|e| e.into())
        })
        .map_err(|e| e.into())
}
