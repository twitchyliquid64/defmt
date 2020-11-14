use core::{
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

use super::SIZE;

pub struct Buffer {
    pub buffer: *mut u8,
    pub write: AtomicUsize,
    pub read: AtomicUsize,
}

impl Buffer {
    pub fn write(&self, bytes: &[u8]) -> usize {
        let read = self.read.load(Ordering::Relaxed);
        let write = self.write.load(Ordering::Acquire);
        let available = if read > write {
            read - write - 1
        } else if read == write {
            SIZE - 1
        } else {
            SIZE - (write - read) - 1
        };
        if available == 0 {
            return 0;
        }

        let cursor = write;
        let len = bytes.len().min(available);

        unsafe {
            if cursor + len > SIZE {
                // split memcpy
                let pivot = SIZE - cursor;
                ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    self.buffer.add(cursor.into()),
                    pivot.into(),
                );
                ptr::copy_nonoverlapping(
                    bytes.as_ptr().add(pivot.into()),
                    self.buffer,
                    (len - pivot).into(),
                );
            } else {
                // single memcpy
                ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    self.buffer.add(cursor.into()),
                    len.into(),
                );
            }
        }
        self.write
            .store(write.wrapping_add(len) % SIZE, Ordering::Release);

        len
    }

    fn read(&mut self, buffer: &mut [u8]) -> usize {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        let available = if read < write {
            write - read
        } else if read == write {
            0
        } else {
            // read > write
            (SIZE - read) + write
        };
        if available == 0 {
            return 0;
        }

        let cursor = read;
        let len = buffer.len().min(available);

        unsafe {
            if cursor + len > SIZE {
                // split memcpy
                let pivot = SIZE - cursor;
                ptr::copy_nonoverlapping(
                    self.buffer.add(cursor.into()),
                    buffer.as_mut_ptr(),
                    pivot.into(),
                );
                ptr::copy_nonoverlapping(
                    self.buffer,
                    buffer.as_mut_ptr().add(pivot.into()),
                    (len - pivot).into(),
                );
            } else {
                // single memcpy
                ptr::copy_nonoverlapping(
                    self.buffer.add(cursor.into()),
                    buffer.as_mut_ptr(),
                    len.into(),
                );
            }
        }
        self.read
            .store(read.wrapping_add(len) % SIZE, Ordering::Release);

        len
    }
}

unsafe impl super::Reader for Buffer {
    fn read(&mut self, buffer: &mut [u8]) -> usize {
        self.read(buffer)
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use crate::buffer::*;
    use crate::Reader;
    use core::fmt::Write;
    use std::vec::Vec;

    #[test]
    fn test_writes() {
        let mut backing_buffer: [u8; SIZE] = [0; SIZE];
        let buffer = Buffer {
            buffer: &mut backing_buffer as *mut _ as *mut u8,
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        };

        assert_eq!(buffer.write(&[0u8, 1u8, 2u8, 3u8]), 4);
        assert_eq!(buffer.write(&['x' as u8; SIZE - 4]), SIZE - 5);
        assert_eq!(buffer.write(&[0u8, 1u8, 2u8, 3u8]), 0);
    }

    #[test]
    fn test_no_wrap() {
        let mut backing_buffer: [u8; SIZE] = [0; SIZE];
        let mut buffer = Buffer {
            buffer: &mut backing_buffer as *mut _ as *mut u8,
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        };

        let mut data = [0; SIZE - 4];
        for (i, b) in data.iter_mut().enumerate() {
            *b = i as u8
        }
        assert_eq!(buffer.write(&data), SIZE - 4);

        let mut read: Vec<u8> = vec![0; SIZE];
        assert_eq!(buffer.read(&mut read), SIZE - 4);
        assert_eq!(buffer.read(&mut read), 0);

        assert_eq!(read[..SIZE - 4], data);
        assert_eq!(
            buffer.read.load(Ordering::Relaxed),
            buffer.write.load(Ordering::Relaxed)
        );
    }

    #[test]
    fn test_wrapping() {
        let mut backing_buffer: [u8; SIZE] = [0; SIZE];
        let mut buffer = Buffer {
            buffer: &mut backing_buffer as *mut _ as *mut u8,
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        };

        // Write SIZE/2 and read it back.
        let mut data1 = [0; SIZE / 2];
        for (i, b) in data1.iter_mut().enumerate() {
            *b = i as u8
        }
        assert_eq!(buffer.write(&data1), SIZE / 2);
        let mut read1: Vec<u8> = vec![0; SIZE / 2];
        assert_eq!(buffer.read(&mut read1), SIZE / 2);
        assert_eq!(buffer.read(&mut read1), 0);
        assert_eq!(read1[..SIZE / 2], data1);
        assert_eq!(
            buffer.read.load(Ordering::Relaxed),
            buffer.write.load(Ordering::Relaxed)
        );

        // Write SIZE-4 and read it back.
        let mut data = [0; SIZE];
        for (i, b) in data.iter_mut().enumerate() {
            *b = i as u8
        }
        assert_eq!(buffer.write(&data), SIZE - 1);
        let mut read: Vec<u8> = vec![0; SIZE];
        assert_eq!(buffer.read(&mut read), SIZE - 1);
        assert_eq!(buffer.read(&mut read), 0);
        assert_eq!(read[..SIZE - 1], data[..SIZE - 1]);
        assert_eq!(
            buffer.read.load(Ordering::Relaxed),
            buffer.write.load(Ordering::Relaxed)
        );
    }
}
