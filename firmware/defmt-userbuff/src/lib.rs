//! [`defmt`](https://github.com/knurling-rs/defmt) global logger serviced by user code.
//!
//! To use this crate, link to it by importing it somewhere in your project.
//!
//! ```
//! // src/main.rs or src/bin/my-app.rs
//! use defmt_userbuff as _;
//! ```

#![no_std]

use core::{
    ptr::NonNull,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use cortex_m::{interrupt, register};
//
// use usb_device::UsbBus;
// use usbd_hid::descriptor::generator_prelude::*;
// use usbd_hid::hid_class::HIDClass;

mod buffer;
use buffer::Buffer;

// TODO make configurable
// NOTE use a power of 2 for best performance
const SIZE: usize = 1024;

#[defmt::global_logger]
struct Logger;

impl defmt::Write for Logger {
    fn write(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let consumed = unsafe { handle().write(bytes) };
            if consumed != 0 {
                bytes = &bytes[consumed..];
            }
        }
    }
}

static TAKEN: AtomicBool = AtomicBool::new(false);
static INTERRUPTS_ACTIVE: AtomicBool = AtomicBool::new(false);

unsafe impl defmt::Logger for Logger {
    fn acquire() -> Option<NonNull<dyn defmt::Write>> {
        let primask = register::primask::read();
        interrupt::disable();
        if !TAKEN.load(Ordering::Relaxed) {
            // no need for CAS because interrupts are disabled
            TAKEN.store(true, Ordering::Relaxed);

            INTERRUPTS_ACTIVE.store(primask.is_active(), Ordering::Relaxed);

            Some(NonNull::from(&Logger as &dyn defmt::Write))
        } else {
            if primask.is_active() {
                // re-enable interrupts
                unsafe { interrupt::enable() }
            }
            None
        }
    }

    unsafe fn release(_: NonNull<dyn defmt::Write>) {
        TAKEN.store(false, Ordering::Relaxed);
        if INTERRUPTS_ACTIVE.load(Ordering::Relaxed) {
            // re-enable interrupts
            interrupt::enable()
        }
    }
}

pub unsafe trait Reader {
    /// Services the log buffer by consuming unread data into buffer.
    fn read(&mut self, buffer: &mut [u8]) -> usize;
}

// struct UsbInner {
//     hid: Option<Option<HIDClass<UsbBus>>>,
//     buff: Buffer,
// }
// #[gen_hid_descriptor(
//     (collection = 0x01, usage = 0x01, usage_page = 0xff00) = {
//         cmd=input;
//         cmd=output;
//     }
// )]
// #[allow(dead_code)]
// struct CommandPacket {
//     cmd: u32,
// }

// make sure we only get shared references to the header/channel (avoid UB)
/// # Safety
/// `Channel` API is not re-entrant; this handle should not be held from different execution
/// contexts (e.g. thread-mode, interrupt context)
unsafe fn handle() -> &'static Buffer {
    #[no_mangle]
    static mut _BUFFER: Buffer = Buffer {
        buffer: unsafe { &mut BUFFER as *mut _ as *mut u8 },
        write: AtomicUsize::new(0),
        read: AtomicUsize::new(0),
    };

    #[link_section = ".uninit.defmt-userbuff.BUFFER"]
    static mut BUFFER: [u8; SIZE] = [0; SIZE];

    &_BUFFER
}

#[cfg(test)]
#[macro_use]
extern crate std;
