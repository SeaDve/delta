use anyhow::Result;
use gtk::gio::{self, prelude::*};

pub fn ensure_file_parents(file: &gio::File) -> Result<()> {
    if let Err(err) = file
        .parent()
        .unwrap()
        .make_directory_with_parents(gio::Cancellable::NONE)
    {
        if !err.matches(gio::IOErrorEnum::Exists) {
            return Err(err.into());
        }
    }

    Ok(())
}
