pub const FALSE: i32 = 0;

#[derive(Default, Debug)]
pub struct HidDeviceInfo {
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: String,
    pub release_number: u16,
    pub manufacturer_string: String,
    pub product_string: String,
    pub usage_page: u16,
    pub usage: u16,
    pub interface_number: i32,
}

#[macro_use]
macro_rules! bool_check {
    ($res: ident) => {
        if $res == FALSE {
            break;
        }
    };
}
