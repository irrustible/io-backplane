use blocking::unblock;
use crate::buffer::{Buffer, Readable, Writeable};

use std::cmp::min;
use std::fs;
use std::io::{self, Error, ErrorKind};
use std::mem::{ManuallyDrop, replace};
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};

#[cfg(all(unix, not(any(
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "linux",
))))]
use std::os::unix::fs::FileExt;

#[derive(Clone)]
pub struct IO {}

impl IO {
    pub async fn open_file(path: PathBuf, opts: fs::OpenOptions) -> Result<File, Error> {
        Ok(File(unblock(move || opts.open(path)).await?))
    }
}

pub struct File(fs::File);

pub struct ReadBuffer {
    buffer: Buffer,
    high: usize,
    low: usize,
}

impl ReadBuffer {
    pub fn new() -> ReadBuffer {
        ReadBuffer::from_buffer(Buffer::new())
    }
 
    pub fn with_capacity(bytes: usize) -> Result<ReadBuffer, Error> {
        Ok(ReadBuffer::from_buffer(Buffer::with_capacity(bytes)?))
    }

    pub fn from_buffer(buffer: Buffer) -> ReadBuffer {
        ReadBuffer { buffer, high: 0, low: 0 }
    }

    pub fn clear(&mut self) {
        self.high = 0;
        self.low = 0;
    }

    pub fn consume(&mut self, bytes: usize) {
        self.low = min(self.low + bytes, self.high);
        if self.low == self.high { self.clear(); }
    }

    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "linux",
    ))]
    pub async fn fill_at(&mut self, file: &File, offset: usize) -> Result<usize, Error> {
        let buf = replace(&mut self.buffer, Buffer::new());
        let high = self.high;
        let fd = file.0.as_raw_fd();
        let (buf2, read) = unblock(move || {
            let mut buf = buf;
            let writeable = Writeable::new(&mut buf, high);
            let mut bufs = Vec::new();
            for w in writeable {
                match w {
                    Ok(w) => { bufs.push(io::IoSliceMut::new(&mut w[..])); }
                    Err(e) => { return (buf, Err(e)); }
                }
            }
            match read_vectored_at(fd, &mut bufs[..], offset) {
                Ok(count) => (buf, Ok(count)),
                Err(e) => (buf, Err(e)),
            }
        }).await;
        #[allow(unused_must_use)]
        { replace(&mut self.buffer, buf2); }
        let read = read?;
        self.high += read;
        Ok(read)
    }

    #[cfg(all(unix,not(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "linux",
    ))))]
    pub async fn fill_at(&mut self, file: &File, pos: usize) -> Result<usize, Error> {
        let buf = replace(&mut self.buffer, Buffer::new());
        let high = self.high;
        let fd = file.0.as_raw_fd();
        let (buf2, read) = unblock(move || {
            let mut buf = buf;
            let file = ManuallyDrop::new(unsafe { fs::File::from_raw_fd(fd) });
            let mut writeable = Writeable::new(&mut buf, high);
            match writeable.next_slice() {
                Ok(w) => {
                    match file.read_at(w, pos as u64) {
                        Ok(count) => (buf, Ok(count)),
                        Err(err) => (buf, Err(err)),
                    }
                }
                Err(e) => (buf, Err(e)),
            }
        }).await;
        #[allow(unused_must_use)]
        { replace(&mut self.buffer, buf2); }
        let read = read?;
        self.high += read;
        Ok(read)
    }
}

pub struct WriteBuffer {
    buffer: Buffer,
    high: usize,
    low: usize,
}

impl WriteBuffer {
    pub fn new() -> WriteBuffer {
        WriteBuffer::from_buffer(Buffer::new())
    }
 
    pub fn with_capacity(bytes: usize) -> Result<WriteBuffer, Error> {
        Ok(WriteBuffer::from_buffer(Buffer::with_capacity(bytes)?))
    }

    pub fn from_buffer(buffer: Buffer) -> WriteBuffer {
        WriteBuffer { buffer, high: 0, low: 0 }
    }

    pub fn clear(&mut self) {
        self.high = 0;
        self.low = 0;
    }

    pub fn buffer(&mut self, buf: &[u8]) -> Result<usize, Error> {
        if buf.is_empty() { return Ok(0) }
        let mut buf = buf;
        let mut wrote: usize = 0;
        let mut wr = Writeable::new(&mut self.buffer, self.high);
        let mut w = Some(wr.next_slice()?);
        loop {
            let bl = buf.len();
            let w2 = w.take().unwrap();
            let wl = w2.len();
            if bl > wl {
                {
                    let w3 = w2;
                    w3.copy_from_slice(&buf[..wl]);
                }
                buf = &buf[wl..];
                wrote += wl;
                match wr.next_slice() {
                    Ok(slice) => { w = Some(slice); }
                    Err(e) => {
                        self.high += wrote;
                        return Err(e);
                    }
                }
            } else {
                if wl > bl {
                    (&mut w2[..bl]).copy_from_slice(buf);
                } else {
                    w2.copy_from_slice(buf);
                }
                wrote += bl;
                break;
            }
        }
        self.high += wrote;
        Ok(wrote)
    }

    pub fn len(&self) -> usize {
        self.high - self.low
    }

    // when pwritev is available, we can make fewer syscalls!
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "linux",
    ))]
    pub async fn write_at(&mut self, file: &File, offset: usize, sync: bool) -> Result<(), Error> {
        let buf = replace(&mut self.buffer, Buffer::new());
        let high = self.high;
        let low = self.low;
        let fd = file.0.as_raw_fd();
        let (buf2, count) = unblock(move || {
            let mut buf = buf;
            let readable = Readable::new(&mut buf, low, high - low);
            let mut bufs: Vec<io::IoSlice> = readable.map(|s| io::IoSlice::new(&s[..])).collect();
            if bufs.is_empty() {
                Ok((buf, 0))
            } else {
                let count = write_vectored_at(fd, bufs.as_mut_slice(), offset)?;
                if sync {
                    ManuallyDrop::new(unsafe { fs::File::from_raw_fd(fd) }).sync_data()?;
                }
                Ok::<_, Error>((buf, count))
            }
        }).await?;
        #[allow(unused_must_use)]
        { replace(&mut self.buffer, buf2); }
        self.low += count;
        if self.low == self.high {
            self.clear();
        }
        Ok(())
    }

    // when pwritev is available, we can make fewer syscalls!
    #[cfg(any(
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "macos",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "linux",
    ))]
    pub async fn write_all_at(&mut self, file: &File, offset: usize, sync: bool) -> Result<(), Error> {
        let buf = replace(&mut self.buffer, Buffer::new());
        let high = self.high;
        let low = self.low;
        let fd = file.0.as_raw_fd();
        let buf2 = unblock(move || {
            let mut buf = buf;
            let readable = Readable::new(&mut buf, low, high - low);
            let mut bufs: Vec<io::IoSlice> = readable.map(|s| io::IoSlice::new(&s[..])).collect();
            write_all_vectored_at(fd, bufs.as_mut_slice(), offset)?;
            if sync {
                ManuallyDrop::new(unsafe { fs::File::from_raw_fd(fd) }).sync_data()?;
            }
            Ok::<_, Error>(buf)
        }).await?;
        #[allow(unused_must_use)]
        { replace(&mut self.buffer, buf2); }
        self.clear();
        Ok(())
    }

    #[cfg(all(
        unix,
        not(any(
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "macos",
            target_os = "netbsd",
            target_os = "openbsd",
            target_os = "linux",
        ))))]
    pub async fn write_all_at(mut self, file: &fs::File, offset: usize, sync: bool) -> Result<(), Error> {
        let buf = replace(&mut self.buffer, Buffer::new());
        let high = self.high;
        let low = self.low;
        let fd = file.as_raw_fd();
        let buf2 = unblock(move || {
            let mut buf = buf;
            let file = ManuallyDrop::new(unsafe { fs::File::from_raw_fd(fd) });
            let mut wrote: usize = 0;
            let mut readable = Readable::new(&mut buf, low, high - low);
            for r in readable {
                loop {
                    if let Err(err) = file.write_all_at(r, (offset + low) as u64) {
                        if err.kind() != ErrorKind::Interrupted { return Err(err); }
                    } else {
                        low += r.len();
                        break;
                    }
                }
            }
            if sync { file.sync_data()?; }
            Ok(buf)
        }).await?;
        #[allow(unused_must_use)]
        { replace(&mut self.buffer, buf2); }
        self.clear();
        Ok(())
    }
}

use libc;

#[cfg(any(
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd",
))]
const MAX_IOV: usize = libc::IOV_MAX as usize;

#[cfg(target_os = "linux")]
const MAX_IOV: usize = libc::UIO_MAXIOV as usize;

#[cfg(any(
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "linux",
))]
fn write_vectored_at(fd: RawFd, bufs: &[io::IoSlice], offset: usize) -> Result<usize, Error> {
    let ret = unsafe {
        libc::pwritev(
            fd,
            bufs.as_ptr() as *const libc::iovec,
            min(bufs.len(), MAX_IOV) as libc::c_int,
            offset as i64
        )
    };
    if ret as isize == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(ret as usize)
    }    
}

#[cfg(any(
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "linux",
))]

fn write_all_vectored_at(fd: RawFd, mut bufs: &mut [io::IoSlice], offset: usize) -> Result<usize, Error> {
    let mut wrote: usize = 0;
    while !bufs.is_empty() {
        match write_vectored_at(fd, bufs, offset + wrote) {
            Ok(bytes) => {
                wrote += bytes;
                bufs = io::IoSlice::advance(bufs, bytes);
            }
            Err(e) => {
                if e.kind() != ErrorKind::Interrupted {
                    return Err(e);
                }
            }
        }
    }
    Ok(wrote)
}

#[cfg(any(
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "macos",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "linux",
))]
fn read_vectored_at(fd: RawFd, bufs: &mut [io::IoSliceMut], offset: usize) -> Result<usize, Error> {
    let ret = unsafe {
        libc::preadv(
            fd,
            bufs.as_ptr() as *const libc::iovec,
            min(bufs.len(), MAX_IOV) as libc::c_int,
            offset as i64
        )
    };
    if ret as isize == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(ret as usize)
    }    
}
