pub struct DeviceError {
    pub filepath: String,
    pub message: String,
}

impl DeviceError {
    pub fn new(filepath: String, message: String) -> DeviceError {
        return DeviceError {
            filepath,
            message,
        };
    }
}