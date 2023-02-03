use libc::{self, umask};
use libcore::c_str;
use libcore::mount::{early_mount, early_partitions};
use std::convert::TryFrom;
use std::ffi::CStr;
use std::fs::OpenOptions;
use std::io::Write;
use std::os::linux::fs::MetadataExt;
//use std::os::unix::prelude::MetadataExt;
use std::panic;
use std::path::PathBuf;
use std::{alloc::System, ffi::CString, path::Path};

#[global_allocator]
static GLOBAL: System = System;

const SYSTEM_INIT_PATH: *const std::os::raw::c_char = c_str!("/sbin/init");

use std::os::unix::fs::PermissionsExt;
fn main() {
    let start_time = std::time::Instant::now();
    println!("Sabaton INITRD");
    let path_val = CString::new("/bin").unwrap();
    let mut errors = Vec::new();

    let root_dir_at_boot = unsafe { libc::opendir(c_str!("/")) };

    let orig_root_metadata = std::fs::metadata("/").expect("Cannot get metadata of original root");

    unsafe {
        umask(0);
        let err = libc::setenv(c_str!("PATH"), path_val.as_ptr(), 1);
        if 0 != err {
            errors.push(format!("setenv failed:{}", err));
        }
    }

    let errors = early_mount::early_mount();

    if errors.len() > 0 {
        for err in errors {
            println!("{}", err);
        }
    } else {
        println!("No startup errors");
    }

    // Enable proper logging
    simple_logger::SimpleLogger::new()
        .with_utc_timestamps()
        .with_level(log::LevelFilter::Debug)
        .init()
        .unwrap();

    // Use the default implementation of the boothal
    let mut boot_hal = sabaton_hal::bootloader::mock::DefaultImpl;

    let pre_mount_time = std::time::Instant::now();

    early_partitions::mount_early_partitions(&mut boot_hal)
        .expect("Unable to mount early partitions");

    let mount_time = std::time::Instant::now();

    let new_root_metadata = std::fs::metadata("/").expect("Cannot get metadata of new root");

    let device_unique_id = get_device_unique_id().unwrap();
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("/tmp/device_unique_id")
        .expect("Unable to open a writable file");
    file.write(device_unique_id.as_bytes())
        .expect("Unable to write device id to file");
    println!("device_unique_id:{}", device_unique_id);
    let res = libmount::BindMount::new("/tmp/device_unique_id", "/etc/machine-id");
    println!("RES={}", res);
    res.bare_mount().expect("Mount not sucessful!");
    if std::os::linux::fs::MetadataExt::st_dev(&new_root_metadata)
        != std::os::linux::fs::MetadataExt::st_dev(&orig_root_metadata)
    {
        early_mount::cleanup_ramdisk(
            root_dir_at_boot,
            std::os::linux::fs::MetadataExt::st_dev(&orig_root_metadata),
        );
    }

    log::info!("Filesystems mounted");
    log::info!(
        "Pre-Mount:+{:?} Post-Mount:+{:?}",
        pre_mount_time - start_time,
        mount_time - start_time
    );

    // Now we're ready to exec into the second stage init inside the root
    // partition. We expect the init to be in /sbin/init
    // initception does not attempt to mount the early devices when the -n parameter
    // is passed.
    let args = [SYSTEM_INIT_PATH, c_str!("-n"), std::ptr::null()];
    println!("Sabaton INIT Stage 1 complete");
    unsafe {
        libc::execv(SYSTEM_INIT_PATH, args.as_ptr());
    }
    log::error!("Sabaton INITRD : Failed launching system init");
    panic!("Fatal error in stage1 init");
}

// Returns the current version of the target
pub fn get_device_unique_id() -> Result<String, std::io::Error> {
    let device_unique_id = std::fs::read_to_string("/sys/firmware/devicetree/base/serial-number")?;

    Ok(device_unique_id)
}
