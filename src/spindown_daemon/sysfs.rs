use std::fs::read_to_string;

use crate::spindown_daemon::errors::DeviceError;

pub fn get_device_stats(dev: &String) -> Result<(u64, u64), DeviceError> {
    let filename: String = format!("/sys/block/{}/stat", dev);
    let read_result = read_to_string(filename.clone());
    if read_result.is_err() {
        return Err(DeviceError::new(filename, read_result.unwrap_err().to_string()));
    };
    let content = read_result.unwrap();

    // see https://www.kernel.org/doc/Documentation/block/stat.txt
    let mut elements = content.split_whitespace();
    let read_iops = elements.nth(0).unwrap().parse().unwrap();
    let write_iops = elements.nth(4).unwrap().parse().unwrap();
    Ok((read_iops, write_iops))
}