use std::{fmt, io};
use std::fmt::{Display, Formatter};
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::{IntoRawFd, RawFd};
use std::ptr::null_mut;

use nix::libc::{c_int, c_uchar, c_uint, c_ulong, c_ushort, c_void, ioctl, O_NONBLOCK};
use nix::unistd::close;

use crate::spindown_daemon::errors::DeviceError;

const SAT_ATA_PASS_THROUGH16: u8 = 0x85;
const ATA_CHECK_POWER_MODE: u8 = 0xE5;
const ATA_OP_STANDBYNOW: u8 = 0xE0;
const SG_IO: c_ulong = 0x2285;
const SENSE_LEN: usize = 32;

const PROTOCOL: u8 = 3;  /* non-dat data-in */
const EXTEND: u8 = 0;
const CHK_COND: u8 = 1; /* set to 1 to read register(s) back */
const T_DIR: u8 = 1; /* 0 -> to device, 1 -> from device */
const BYTE_BLOCK: u8 = 1; /* 0 -> bytes, 1 -> 512 byte blocks */
const T_LENGTH: u8 = 0; /* 0 -> no data transferred, 2 -> sector count */

#[derive(Debug, PartialEq)]
pub enum PowerState {
    Standby,
    Idle,
    IdleA,
    IdleB,
    IdleC,
    ActiveOrIdle,
    Unknown,
}

impl Display for PowerState {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[repr(C)]
#[derive(Debug)]
struct SgIoHdr {
    // [i] 'S' for SCSI generic (required)
    interface_id: c_int,
    // [i] data transfer direction
    dxfer_direction: c_int,
    // [i] SCSI command length ( <= 16 bytes)
    cmd_len: c_uchar,
    // [i] max length to write to sbp
    mx_sb_len: c_uchar,
    // [i] 0 implies no scatter gather
    iovec_count: c_ushort,
    // [i] byte count of data transfer
    dxfer_len: c_uint,
    // [i], [*io] points to data transfer memory or scatter gather list
    dxferp: *mut c_void,
    // [i], [*i] points to command to perform
    cmdp: *const c_uchar,
    // [i], [*o] points to sense_buffer memory
    sbp: *mut c_uchar,
    // [i] MAX_UINT->no timeout (unit: millisec)
    timeout: c_uint,
    // [i] 0 -> default, see SG_FLAG...
    flags: c_uint,
    // [i->o] unused internally (normally)
    pack_id: c_int,
    // [i->o] unused internally
    usr_ptr: *mut c_void,
    // [o] scsi status
    status: c_uchar,
    // [o] shifted, masked scsi status
    masked_status: c_uchar,
    // [o] messaging level data (optional)
    msg_status: c_uchar,
    // [o] byte count actually written to sbp
    sb_len_wr: c_uchar,
    // [o] errors from host adapter
    host_status: c_ushort,
    // [o] errors from software driver
    driver_status: c_ushort,
    // [o] dxfer_len - actual_transferred
    resid: c_int,
    // [o] time taken by cmd (unit: millisec)
    duration: c_uint,
    // [o] auxiliary information
    info: c_uint,
}

fn exec_sg(dev: &String, command: u8, sense: Option<&mut Vec<u8>>) -> Result<(), DeviceError> {
    let raw_fd = open_dev_raw(dev)?;

    let tmp_sense = &mut vec![0; SENSE_LEN];
    let sbp = sense.unwrap_or(tmp_sense);

    // see https://www.t10.org/ftp/t10/document.04/04-262r8.pdf
    // section 13.2.3 ATA PASS-THROUGH (16) command overview
    let mut cmd: [u8; 16] = [SAT_ATA_PASS_THROUGH16, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0] as [u8; 16];
    cmd[1] = (PROTOCOL << 1) | EXTEND;
    cmd[2] = (CHK_COND << 5) | (T_DIR << 3) |
        (BYTE_BLOCK << 2) | T_LENGTH;
    cmd[14] = command;

    // see https://tldp.org/HOWTO/SCSI-Generic-HOWTO/sg_io_hdr_t.html
    let sg_io_hdr = SgIoHdr {
        interface_id: 'S' as c_int,

        dxfer_direction: -1, // Direction::None
        dxferp: null_mut() as *mut c_void,
        dxfer_len: 0 as c_uint,
        resid: 0,

        sbp: sbp.as_mut_ptr(),
        mx_sb_len: sbp.capacity() as c_uchar,
        sb_len_wr: 0,

        cmdp: cmd.as_ptr(),
        cmd_len: cmd.len() as c_uchar,

        status: 0,
        host_status: 0,
        driver_status: 0,

        timeout: 15000,
        duration: 0,

        iovec_count: 0,
        flags: 0,
        pack_id: 0,
        usr_ptr: null_mut(),
        masked_status: 0,
        msg_status: 0,
        info: 0,
    };

    unsafe {
        if ioctl(raw_fd, SG_IO, &sg_io_hdr) != 0 {
            match close(raw_fd) {
                Ok(()) => (),
                Err(e) => println!("unable to close {}: {}", dev.to_string(), e.to_string())
            }
            return Err(DeviceError::new(dev.to_string(), io::Error::last_os_error().to_string()));
        }
    }
    match close(raw_fd) {
        Ok(()) => (),
        Err(e) => return Err(DeviceError::new(dev.to_string(), e.to_string()))
    }
    Ok(())
}

fn open_dev_raw(dev: &String) -> Result<RawFd, DeviceError> {
    let mut options = OpenOptions::new();
    options.read(true);

    if cfg!(unix) {
        options.custom_flags(O_NONBLOCK);
    }

    match options.open(format!("/dev/{}", dev)) {
        Ok(fd) => { Ok(fd.into_raw_fd()) }
        Err(e) => { Err(DeviceError::new(dev.to_string(), e.to_string())) }
    }
}

pub fn check_power_state(dev: &String) -> Result<PowerState, DeviceError> {
    let mut sense = vec![0; SENSE_LEN];
    exec_sg(dev, ATA_CHECK_POWER_MODE, Option::Some(&mut sense))?;

    let power_status = match sense[13] {
        0x00 => PowerState::Standby,
        0x80 => PowerState::Idle,
        0x81 => PowerState::IdleA,
        0x82 => PowerState::IdleB,
        0x83 => PowerState::IdleC,
        0xFF => PowerState::ActiveOrIdle,
        _ => PowerState::Unknown,
    };
    Ok(power_status)
}

pub fn do_standby(dev: &String) -> Result<(), DeviceError> {
    exec_sg(dev, ATA_OP_STANDBYNOW, Option::None)?;
    Ok(())
}