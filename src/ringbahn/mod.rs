use futures_lite::io::{AsyncBufRead, AsyncRead, AsyncSeek, AsyncWrite};
use maglev::Driver;
use ringbahn::fs::{self, AsyncWriteExt};
use std::io::{IoSlice, IoSliceMut, Result, SeekFrom};
use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Clone)]
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

impl AsyncBufRead for File {
    fn poll_fill_buf(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<&[u8]>> {
        let this = Pin::into_inner(self);
        fs::File::poll_fill_buf(Pin::new(&mut this.0), ctx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let this = Pin::into_inner(self);
        fs::File::consume(Pin::new(&mut this.0), amt)
    }
}

impl AsyncRead for File {
    fn poll_read(self: Pin<&mut Self>, ctx: &mut Context, buf: &mut [u8]) -> Poll<Result<usize>> {
        let this = Pin::into_inner(self);
        fs::File::poll_read(Pin::new(&mut this.0), ctx, buf)
    }

    fn poll_read_vectored(self: Pin<&mut Self>, ctx: &mut Context, bufs: &mut [IoSliceMut]) -> Poll<Result<usize>> {
        let this = Pin::into_inner(self);
        fs::File::poll_read_vectored(Pin::new(&mut this.0), ctx, bufs)
    }
}

impl AsyncSeek for File {
    fn poll_seek(self: Pin<&mut Self>, ctx: &mut Context, pos: SeekFrom) -> Poll<Result<u64>> {
        let this = Pin::into_inner(self);
        fs::File::poll_seek(Pin::new(&mut this.0), ctx, pos)
    }
}

impl AsyncWrite for File {
    fn poll_write(self: Pin<&mut Self>, ctx: &mut Context, buf: &[u8]) -> Poll<Result<usize>> {
        let this = Pin::into_inner(self);
        fs::File::poll_write(Pin::new(&mut this.0), ctx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<()>> {
        let this = Pin::into_inner(self);
        fs::File::poll_flush(Pin::new(&mut this.0), ctx)
    }

    fn poll_close(self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Result<()>> {
        let this = Pin::into_inner(self);
        fs::File::poll_close(Pin::new(&mut this.0), ctx)
    }

    fn poll_write_vectored(self: Pin<&mut Self>, ctx: &mut Context, bufs: &[IoSlice]) -> Poll<Result<usize>> {
        let this = Pin::into_inner(self);
        fs::File::poll_write_vectored(Pin::new(&mut this.0), ctx, bufs)
    }
}

impl AsyncWriteExt for File {
    fn poll_pwrite(self: Pin<&mut Self>, ctx: &mut Context, slice: &[u8], pos: usize) -> Poll<Result<usize>> {
        let this = Pin::into_inner(self);
        fs::File::poll_pwrite(Pin::new(&mut this.0), ctx, slice, pos)
    }
}
