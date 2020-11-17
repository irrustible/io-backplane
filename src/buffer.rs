use concurrent_queue::ConcurrentQueue;
use once_cell::sync::Lazy;
use smallvec::SmallVec;
use crate::mmap::MmapMut;

use std::cmp::min;
use std::cell::UnsafeCell;
use std::io::Error;
use std::mem::replace;

pub(crate) const PAGE_SIZE: usize = 4096;

struct FreeList {
    queue: ConcurrentQueue<MmapMut>,
}

static FREE_LIST: Lazy<FreeList> = Lazy::new(|| {
    FreeList { queue: ConcurrentQueue::unbounded() }
});

impl FreeList {
    fn pop(&self) -> Option<MmapMut> {
        FREE_LIST.queue.pop().ok()
    }
    #[allow(unused_must_use)]
    fn push(&self, item: MmapMut) {
        FREE_LIST.queue.push(item);
    }
}

fn page() -> Result<MmapMut, Error> {
    if let Some(page) = FREE_LIST.pop() {
        Ok(page)
    } else {
        MmapMut::anon(PAGE_SIZE, true)
    }
}

fn page_out(page: MmapMut) {
    FREE_LIST.push(page);
}

pub struct Buffer {
    pub(crate) buffer: SmallVec<[UnsafeCell<MmapMut>; 2]>,
}

impl Buffer {
    pub fn new() -> Buffer {
        Buffer{ buffer: SmallVec::new() }
    }

    pub fn capacity(&self) -> usize {
        self.buffer.len() * PAGE_SIZE
    }

    pub fn with_pages(pages: usize) -> Result<Buffer, Error> {
        let mut buffer = SmallVec::with_capacity(pages);
        for _ in 0..pages {
            buffer.push(UnsafeCell::new(page()?));
        }
        Ok(Buffer { buffer })
    }

    pub fn with_capacity(bytes: usize) -> Result<Buffer, Error> {
        let div = bytes / PAGE_SIZE;
        let rem = bytes % PAGE_SIZE;
        Buffer::with_pages(if rem == 0 { div } else { div + 1 })
    }

    pub fn reserve(&mut self, bytes: usize) -> Result<(), Error> {
        let cap = self.capacity();
        if cap > bytes {
            Ok(())
        } else {
            self.reserve_extra(bytes - cap)
        }
    }

    pub fn reserve_extra(&mut self, bytes: usize) -> Result<(), Error> {
        let div = bytes / PAGE_SIZE;
        let blocks = if (bytes % PAGE_SIZE) == 0 { div } else { div + 1 };
        for _ in 0..blocks {
            self.buffer.push(UnsafeCell::new(page()?));
        }
        Ok(())
    }
    
    pub fn add_page(&mut self) -> Result<(), Error> {
        self.buffer.push(UnsafeCell::new(page()?));
        Ok(())
    }

    pub fn read_first(&self, watermark: usize, limit: usize) -> Option<&[u8]> {
        let block = watermark / PAGE_SIZE;
        let offset = watermark % PAGE_SIZE;
        if block >= self.buffer.len() { return None; }
        unsafe {
            Some(&(&*self.buffer[block].get())[offset..min(offset + limit, PAGE_SIZE)])
        }
    }

    pub fn read_block(&self, block: usize, limit: usize) -> Option<&[u8]> {
        if block >= self.buffer.len() { return None; }
        unsafe {
            Some(&(&*self.buffer[block].get())[..min(limit, PAGE_SIZE)])
        }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        for mmap in replace(&mut self.buffer, SmallVec::new()) {
            page_out(mmap.into_inner());
        }
    }
}

pub(crate) struct Readable<'a> {
    buffer: &'a Buffer,
    state: RState,
}

enum RState {
    First(usize, usize),
    Rest(usize, usize),
}

impl<'a> Readable<'a> {
    pub(crate) fn new(buffer: &'a mut Buffer, from: usize, len: usize) -> Readable<'a> {
        Readable { buffer, state: RState::First(from, len) }
    }
}
impl<'a> Iterator for Readable<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<&'a [u8]> {
        match self.state {
            RState::First(watermark, limit) => {
                let block = watermark / PAGE_SIZE;
                let offset = watermark % PAGE_SIZE;
                if block >= self.buffer.buffer.len() { return None; }
                let end = min(offset + limit, PAGE_SIZE);
                let len = end - offset;
                self.state = RState::Rest(block + 1, limit - len);
                unsafe {
                    Some(&(&*self.buffer.buffer[block].get())[offset..min(offset + limit, PAGE_SIZE)])
                }
            }
            RState::Rest(block, limit) => {
                if block >= self.buffer.buffer.len() { return None; }
                self.state = RState::Rest(block + 1, limit.saturating_sub(PAGE_SIZE));
                unsafe {
                    Some(&(&*self.buffer.buffer[block].get())[..min(limit, PAGE_SIZE)])
                }
            }
        }
    }
}

pub(crate) struct Writeable<'a> {
    buffer: &'a mut Buffer,
    state: WState,
}

enum WState {
    First(usize),
    Rest(usize),
}

impl<'a> Writeable<'a> {
    pub(crate) fn new(buffer: &'a mut Buffer, from: usize) -> Writeable<'a> {
        Writeable { buffer, state: WState::First(from) }
    }
    pub(crate) fn next_slice(&mut self) -> Result<&'a mut [u8], Error> {
        match self.state {
            WState::First(watermark) => {
                let block = watermark / PAGE_SIZE;
                let offset = watermark % PAGE_SIZE;
                while block >= self.buffer.buffer.len() { self.buffer.add_page()?; }
                self.state = WState::Rest(block + 1);
                unsafe {
                    Ok(&mut(&mut *self.buffer.buffer[block].get())[offset..])
                }
            }
            WState::Rest(block) => {
                if block >= self.buffer.buffer.len() { self.buffer.add_page()?; }
                self.state = WState::Rest(block + 1);
                unsafe {
                    Ok(&mut(&mut *self.buffer.buffer[block].get())[..])
                }
            }
        }
    }
}

impl<'a> Iterator for Writeable<'a> {
    type Item = Result<&'a mut [u8], Error>;
    fn next(&mut self) -> Option<Result<&'a mut [u8], Error>> {
        Some(self.next_slice())
    }
}
