#[macro_use]
mod types;
mod hid;

fn main() {
    println!("{:#?}", crate::hid::get_hid_device_info_list());
}
