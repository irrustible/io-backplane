use maglev::Driver;
use ringbahn::fs::{self, AsyncWriteExt};
use std::io::{IoSlice, IoSliceMut, Result, SeekFrom};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Clone, Default)]
pub struct IO {
    driver: Driver,
}

pub struct File(fs::File<Driver>);

impl IO {
    pub async fn create_file(&mut self, path: impl AsRef<Path>) -> Result<File> {
        Ok(File(fs::File::create_on_driver(path, self.driver.clone()).await?))
    }

    pub async fn open_file(&mut self, path: impl AsRef<Path>) -> Result<File> {
        Ok(File(fs::File::open_on_driver(path, self.driver.clone()).await?))
    }

    pub async fn from_file(&mut self, file: std::fs::File) -> File {
        File(fs::File::run_on_driver(file, self.driver.clone()))
    }
}
