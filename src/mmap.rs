use libc::{
    c_int, c_void, size_t,
    mmap, munmap,
    MAP_ANONYMOUS, MAP_FIXED, MAP_POPULATE, MAP_PRIVATE,
    PROT_READ, PROT_WRITE,
};
#[cfg(target_os = "linux")]
use libc::mremap;
use std::convert::{AsRef, AsMut};
use std::io::Error;
use std::fs::File;
use std::ops::{Deref, DerefMut};
use std::os::unix::io::AsRawFd;
use std::ptr::null_mut;
use std::slice;

#[derive(Debug)]
pub struct Mmap {
    pub(crate) ptr: *mut u8,
    pub(crate) size: usize,
    pub(crate) unmapped: bool,
    pub(crate) flags: c_int,
}

impl Mmap {
    pub fn file(file: &File, bytes: usize, offset: usize, populate: bool) -> Result<Mmap, Error> {
        let flags = {
            if populate { MAP_ANONYMOUS | MAP_POPULATE | MAP_PRIVATE }
            else { MAP_ANONYMOUS | MAP_PRIVATE }
        };
        Mmap::new(bytes, flags, file.as_raw_fd() as c_int, offset)
    }

    pub fn size(&self) -> usize { self.size }

    #[cfg(target_os = "linux")]
    pub fn remap_in_place(&mut self) -> Result<(), Error> {
        memory_remap_to(self.ptr.cast(), self.size, self.ptr.cast(), self.size, self.flags | MAP_FIXED)?;
        Ok(())
    }

    // #[cfg(not(target_os = "linux"))]
    // pub fn remap_in_place(&mut self) -> Result<(), Error> {
    //     memory_remap_to(self.ptr.cast(), self.size, self.ptr.cast(), self.size, self.flags | MAP_FIXED)?;
    //     Ok(())
    // }

    pub fn close(mut self) -> Result<(), Error> {
        self.unmapped = true;
        memory_unmap(self.ptr, self.size)
    }

    fn new(bytes: usize, flags: c_int, file: c_int, offset: usize) -> Result<Mmap, Error> {
        let null = null_mut::<c_void>().cast();
        let ptr = memory_map(null, bytes, PROT_READ, flags, file, offset as i64)?;
        Ok(Mmap { ptr: ptr.cast(), size: bytes, unmapped: false, flags })
    }
}

impl AsRef<[u8]> for Mmap {
    fn as_ref(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr, self.size) }
    }
}

impl Deref for Mmap {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_ref()
    }
}

impl Drop for Mmap {
    fn drop(&mut self) {
        #[allow(unused_must_use)]
        if !self.unmapped {
            memory_unmap(self.ptr, self.size);
        }
    }
}

#[derive(Debug)]
pub struct MmapMut {
    pub(crate) ptr: *mut u8,
    pub(crate) size: usize,
    pub(crate) unmapped: bool,
    pub(crate) flags: c_int,
}

impl MmapMut {
    pub fn anon(bytes: usize, populate: bool) -> Result<MmapMut, Error> {
        let flags = {
            if populate { MAP_ANONYMOUS | MAP_POPULATE | MAP_PRIVATE }
            else { MAP_ANONYMOUS | MAP_PRIVATE }
        };
        MmapMut::new(bytes, flags, -1, 0)
    }

    pub fn file(file: &File, bytes: usize, offset: usize, populate: bool) -> Result<MmapMut, Error> {
        let flags = {
            if populate { MAP_ANONYMOUS | MAP_POPULATE | MAP_PRIVATE }
            else { MAP_ANONYMOUS | MAP_PRIVATE }
        };
        MmapMut::new(bytes, flags, file.as_raw_fd() as c_int, offset)
    }

    #[cfg(target_os = "linux")]
    pub fn remap_in_place(&mut self) -> Result<(), Error> {
        memory_remap_to(self.ptr.cast(), self.size, self.ptr.cast(), self.size, self.flags | MAP_FIXED)?;
        Ok(())
    }

    pub fn size(&self) -> usize { self.size }

    pub fn close(mut self) -> Result<(), Error> {
        self.unmapped = true;
        memory_unmap(self.ptr, self.size)
    }

    fn new(bytes: usize, flags: c_int, file: c_int, offset: usize) -> Result<MmapMut, Error> {
        let null = null_mut::<c_void>().cast();
        let ptr = memory_map(null, bytes, PROT_READ | PROT_WRITE, flags, file, offset as i64)?;
        Ok(MmapMut { ptr: ptr.cast(), size: bytes, unmapped: false, flags })
    }
}

unsafe impl Send for MmapMut {}

impl AsRef<[u8]> for MmapMut {
    fn as_ref(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr, self.size) }
    }
}

impl AsMut<[u8]> for MmapMut {
    fn as_mut(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

impl Deref for MmapMut {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_ref()
    }
}

impl DerefMut for MmapMut {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.as_mut()
    }
}

impl Drop for MmapMut {
    fn drop(&mut self) {
        #[allow(unused_must_use)]
        if !self.unmapped {
            memory_unmap(self.ptr, self.size);
        }
    }
}

fn memory_map(
    ptr: *mut c_void,
    bytes: usize,
    prot: c_int,
    flags: c_int,
    file: c_int,
    offset: i64
) -> Result<*mut c_void, Error> {
    let ret = unsafe { mmap(ptr, bytes as size_t, prot, flags, file, offset) };
    if ret as isize == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(ret)
    }
}

// #[cfg(target_os = "linux")]
// fn memory_remap(
//     old_ptr: *mut c_void,
//     old_size: usize,
//     new_size: usize,
//     flags: c_int
// ) -> Result<*mut c_void, Error> {
//     let ret = unsafe { mremap(old_ptr, old_size as size_t, new_size as size_t, flags) };
//     if ret as isize == -1 {
//         Err(Error::last_os_error())
//     } else {
//         Ok(ret)
//     }
// }

#[cfg(target_os = "linux")]
fn memory_remap_to(
    old_ptr: *mut c_void,
    old_size: usize,
    new_ptr: *mut c_void,
    new_size: usize,
    flags: c_int
) -> Result<*mut c_void, Error> {
    let ret = unsafe { mremap(old_ptr, old_size as size_t, new_size as size_t, flags, new_ptr) };
    if ret as isize == -1 {
        Err(Error::last_os_error())
    } else {
        Ok(ret)
    }
}

fn memory_unmap(ptr: *mut u8, size: usize) -> Result<(), Error> {
    match unsafe { munmap(ptr.cast(), size as size_t) } {
        -1 => Ok(()),
        _ => Err(Error::last_os_error()),
    }
}

