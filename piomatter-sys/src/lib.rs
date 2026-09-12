//! Safe wrapper over Adafruit Piomatter's HUB75 driver for the Pi 5.
//!
//! Piomatter keeps a reference to the framebuffer for its whole life
//! and re-reads it on every `show()`, so the wrapper owns the buffer
//! and copies each frame into it. The driver spawns its own refresh
//! thread and holds every GPIO the pinout uses, so one instance may
//! exist per Pi. Frames are 32-bit words of the form 0x00RRGGBB, which
//! is what little-endian BGRA bytes are.
//!
//! Building this crate links Piomatter's GPL-2.0-only core; a binary
//! that includes it is GPL-2.0-only when distributed.

use std::ffi::{CStr, c_char};
use std::path::Path;
use std::ptr::NonNull;

/// The RP1 PIO device piolib opens; piolib calls exit(1) if it cannot,
/// so it is checked here first.
pub const PIO_DEVICE: &str = "/dev/pio0";

#[repr(C)]
struct RawConfig {
    width: u32,
    height: u32,
    n_addr_lines: u32,
    n_planes: u32,
    n_temporal_planes: u32,
    n_lanes: u32,
    pinout: u32,
    map: *const i32,
    map_len: usize,
}

#[repr(C)]
struct RawHandle {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn piomatter_create(
        cfg: *const RawConfig,
        framebuffer: *const u32,
        framebuffer_len: usize,
        err: *mut c_char,
        err_len: usize,
    ) -> *mut RawHandle;
    fn piomatter_show(h: *mut RawHandle) -> i32;
    fn piomatter_fps(h: *const RawHandle) -> f64;
    fn piomatter_destroy(h: *mut RawHandle);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum Pinout {
    AdafruitMatrixBonnet = 0,
    AdafruitMatrixBonnetBgr = 1,
    Active3 = 2,
    Active3Bgr = 3,
}

/// Driver configuration. `width` and `height` are the framebuffer
/// dimensions; `map` lists, for every physical shift-register slot in
/// [address][x][lane] order, the framebuffer pixel index it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub width: u32,
    pub height: u32,
    pub n_addr_lines: u32,
    pub n_planes: u32,
    pub n_temporal_planes: u32,
    pub n_lanes: u32,
    pub pinout: Pinout,
    pub map: Vec<i32>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{PIO_DEVICE} is missing: this is not a Pi 5, or the RP1 PIO driver is not loaded")]
    DeviceMissing,
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("driver refused the configuration: {0}")]
    Driver(String),
    #[error("show failed with errno {0}")]
    Show(i32),
    #[error("frame is {got} bytes, expected {expected}")]
    FrameSize { got: usize, expected: usize },
}

/// The framebuffer map for lanes stacked top to bottom: lane `l`
/// covers rows `[l << n_addr_lines, (l + 1) << n_addr_lines)` and each
/// connector is two consecutive lanes. Matches Piomatter's Python
/// `simple_multilane_mapper`.
pub fn multilane_map(width: u32, height: u32, n_addr_lines: u32, n_lanes: u32) -> Vec<i32> {
    let rows_per_lane = 1u32 << n_addr_lines;
    debug_assert_eq!(
        height,
        rows_per_lane * n_lanes,
        "height must be lanes << address lines"
    );
    let mut map = Vec::with_capacity((width * height) as usize);
    for addr in 0..rows_per_lane {
        for x in 0..width {
            for lane in 0..n_lanes {
                map.push(((lane * rows_per_lane + addr) * width + x) as i32);
            }
        }
    }
    map
}

/// One driver instance. Not `Sync`: `show` is unsynchronised in C++.
pub struct Piomatter {
    handle: NonNull<RawHandle>,
    framebuffer: Box<[u32]>,
    width: u32,
    height: u32,
}

// The handle is only ever used from the thread that owns the wrapper.
unsafe impl Send for Piomatter {}

impl Piomatter {
    pub fn new(cfg: &Config) -> Result<Self, Error> {
        if !Path::new(PIO_DEVICE).exists() {
            return Err(Error::DeviceMissing);
        }
        let pixels = cfg.width as usize * cfg.height as usize;
        let lanes_rows = (cfg.n_lanes as usize) << cfg.n_addr_lines;
        if pixels == 0 || lanes_rows == 0 || !pixels.is_multiple_of(lanes_rows) {
            return Err(Error::InvalidConfig(format!(
                "{}x{} framebuffer does not divide into {} lanes of {} address lines",
                cfg.width, cfg.height, cfg.n_lanes, cfg.n_addr_lines
            )));
        }
        if cfg.map.len() != pixels {
            return Err(Error::InvalidConfig(format!(
                "map has {} entries for {pixels} pixels",
                cfg.map.len()
            )));
        }
        if let Some(bad) = cfg.map.iter().find(|&&i| i < 0 || i as usize >= pixels) {
            return Err(Error::InvalidConfig(format!(
                "map entry {bad} is outside the framebuffer"
            )));
        }
        if !(1..=10).contains(&cfg.n_planes) || cfg.n_temporal_planes >= cfg.n_planes {
            return Err(Error::InvalidConfig(
                "n_planes must be 1..=10 and above n_temporal_planes".into(),
            ));
        }
        let framebuffer = vec![0u32; pixels].into_boxed_slice();
        let raw = RawConfig {
            width: cfg.width,
            height: cfg.height,
            n_addr_lines: cfg.n_addr_lines,
            n_planes: cfg.n_planes,
            n_temporal_planes: cfg.n_temporal_planes,
            n_lanes: cfg.n_lanes,
            pinout: cfg.pinout as u32,
            map: cfg.map.as_ptr(),
            map_len: cfg.map.len(),
        };
        let mut err = [0 as c_char; 256];
        // SAFETY: every pointer is valid for the call; the framebuffer
        // is heap-allocated and kept in the returned struct, declared
        // after the handle so it outlives the driver.
        let handle = unsafe {
            piomatter_create(
                &raw,
                framebuffer.as_ptr(),
                framebuffer.len(),
                err.as_mut_ptr(),
                err.len(),
            )
        };
        let handle = NonNull::new(handle).ok_or_else(|| {
            let message = unsafe { CStr::from_ptr(err.as_ptr()) }
                .to_string_lossy()
                .into_owned();
            Error::Driver(message)
        })?;
        Ok(Self {
            handle,
            framebuffer,
            width: cfg.width,
            height: cfg.height,
        })
    }

    /// Copy a BGRA8 frame in and push it to the panels. Blocks while
    /// two earlier frames are still queued.
    pub fn show(&mut self, bgra: &[u8]) -> Result<(), Error> {
        let expected = self.framebuffer.len() * 4;
        if bgra.len() != expected {
            return Err(Error::FrameSize {
                got: bgra.len(),
                expected,
            });
        }
        for (word, px) in self.framebuffer.iter_mut().zip(bgra.chunks_exact(4)) {
            *word = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
        }
        // SAFETY: the handle is live until drop.
        match unsafe { piomatter_show(self.handle.as_ptr()) } {
            0 => Ok(()),
            errno => Err(Error::Show(errno)),
        }
    }

    /// Panel refresh passes per second, as the driver measures them.
    pub fn fps(&self) -> f64 {
        // SAFETY: the handle is live until drop.
        unsafe { piomatter_fps(self.handle.as_ptr()) }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl Drop for Piomatter {
    fn drop(&mut self) {
        // SAFETY: destroyed exactly once; the framebuffer field is
        // dropped afterwards.
        unsafe { piomatter_destroy(self.handle.as_ptr()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_multilane_map_stacks_lanes_top_to_bottom_in_address_x_lane_order() {
        let map = multilane_map(128, 32, 4, 2);
        assert_eq!(map.len(), 128 * 32, "one entry per pixel");
        assert_eq!(map[0], 0, "address 0, x 0, lane 0 is the first pixel");
        assert_eq!(map[1], 16 * 128, "lane 1 starts sixteen rows down");
        assert_eq!(map[2], 1, "next x on lane 0");
        assert_eq!(map[128 * 2], 128, "address 1 is the second row");
        let six = multilane_map(96, 32, 3, 4);
        assert_eq!(
            six[3],
            24 * 96,
            "lane 3 of a 1/8-scan four-lane map starts on row 24"
        );
    }

    #[test]
    fn creating_a_driver_without_the_pio_device_fails_cleanly() {
        if Path::new(PIO_DEVICE).exists() {
            return;
        }
        let cfg = Config {
            width: 128,
            height: 32,
            n_addr_lines: 4,
            n_planes: 8,
            n_temporal_planes: 0,
            n_lanes: 2,
            pinout: Pinout::AdafruitMatrixBonnet,
            map: multilane_map(128, 32, 4, 2),
        };
        assert!(
            matches!(Piomatter::new(&cfg), Err(Error::DeviceMissing)),
            "no /dev/pio0 must be a clean error"
        );
    }
}
