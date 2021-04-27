use std::borrow::Borrow;
use std::time::SystemTime;

use crate::spindown_daemon::ata::{check_power_state, PowerState};
use crate::spindown_daemon::errors::DeviceError;
use crate::spindown_daemon::sysfs::get_device_stats;

pub mod ata;
pub mod sysfs;
pub mod errors;

#[derive(Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub timeout: u64,
    pub power_state: PowerState,
    pub last_read_iops: u64,
    pub last_write_iops: u64,
    pub last_update: SystemTime,
}

pub fn get_device_info(dev: &String) -> Result<DeviceInfo, DeviceError> {
    let device_stats = get_device_stats(dev.borrow())?;
    let power_state = check_power_state(dev.borrow())?;
    Ok(DeviceInfo {
        name: dev.to_string(),
        timeout: 0,
        power_state,
        last_read_iops: device_stats.0,
        last_write_iops: device_stats.1,
        last_update: SystemTime::now(),
    })
}